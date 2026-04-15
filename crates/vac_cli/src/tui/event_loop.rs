//! Event Loop Module

use crate::tui::app::{AppState, AppStateOptions, InputEvent, OutputEvent};
use crate::tui::event::map_crossterm_event_to_input_event;
use crate::tui::services::helper_block::welcome_messages;
use crate::tui::terminal::TerminalGuard;
use crate::tui::view::view;
use crate::tui::Model;
use crossterm::{
    event::{EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::interval;

/// Rulebook configuration
#[derive(Clone, Debug, Default)]
pub struct RulebookConfig {
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
}

/// Run the TUI
#[allow(clippy::too_many_arguments)]
pub async fn run_tui(
    mut input_rx: Receiver<InputEvent>,
    output_tx: Sender<OutputEvent>,
    _cancel_tx: Option<tokio::sync::broadcast::Sender<()>>,
    _shutdown_tx: tokio::sync::broadcast::Sender<()>,
    latest_version: Option<String>,
    _redact_secrets: bool,
    _privacy_mode: bool,
    _is_git_repo: bool,
    _auto_approve_tools: Option<&Vec<String>>,
    _allowed_tools: Option<&Vec<String>>,
    _current_profile_name: String,
    _rulebook_config: Option<RulebookConfig>,
    model: Option<Model>,
    session_id: Option<String>,
    _editor_command: Option<String>,
    _auth_display_info: (Option<String>, Option<String>, Option<String>),
    _init_prompt_content: Option<String>,
    _send_init_prompt_on_start: bool,
    _recent_models: Vec<String>,
    _banner_message: Option<()>,
    project_root: std::path::PathBuf,
) -> io::Result<()> {
    let _guard = TerminalGuard;
    enable_raw_mode()?;
    execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture
    )?;

    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;

    // Check for session restore
    let checkpoint_path = std::env::var("VAC_CHECKPOINT").ok().map(std::path::PathBuf::from);

    let mut state = AppState::new(AppStateOptions {
        model: model.clone(),
        session_id,
        checkpoint_path: checkpoint_path.clone(),
        project_root,
    });

    // Add welcome messages
    let welcome = welcome_messages(latest_version.as_deref(), &state);
    state.messages.extend(welcome);

    // Request session restore if checkpoint exists
    if let Some(path) = &checkpoint_path {
        if let Some(session_id) = path.file_name().and_then(|n| n.to_str()) {
            let _ = output_tx.try_send(OutputEvent::ResumeSession(session_id.to_string()));
        }
    }

    // Create input thread
    let (input_tx, mut internal_rx) = tokio::sync::mpsc::channel::<InputEvent>(100);
    let input_paused = Arc::new(AtomicBool::new(false));
    let input_paused_clone = input_paused.clone();

    std::thread::spawn(move || {
        loop {
            if input_paused_clone.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            if crossterm::event::poll(Duration::from_millis(100)).ok()? {
                if let Some(event) = crossterm::event::read().ok()
                    .and_then(|e| map_crossterm_event_to_input_event(e))
                {
                    if input_tx.blocking_send(event).is_err() {
                        break;
                    }
                }
            }
        }
        Some(())
    });

    let mut spinner_interval = interval(Duration::from_millis(150));

    loop {
        // Handle internal events
        while let Ok(event) = internal_rx.try_recv() {
            handle_input_event(&mut state, &output_tx, event);
        }

        // Handle backend events
        while let Ok(event) = input_rx.try_recv() {
            handle_backend_event(&mut state, &output_tx, event);
        }

        // Update spinner
        spinner_interval.tick().await;
        if state.loading || state.is_streaming {
            state.spinner_frame = (state.spinner_frame + 1) % 10;
        }

        // Render
        terminal.draw(|f| view(f, &mut state))?;

        // Check for quit
        if state.cancel_requested {
            break;
        }
    }

    Ok(())
}

/// Handle input events from user
fn handle_input_event(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) {
    // Handle command palette input first
    if state.show_command_palette {
        match event {
            InputEvent::HandleEsc | InputEvent::HideCommandPalette => {
                state.show_command_palette = false;
                state.command_palette_input.clear();
                state.command_palette_selected = 0;
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
                    match cmd.source {
                        crate::tui::app::CommandSource::BuiltIn => {
                            if cmd.command == "/clear" {
                                state.messages.clear();
                                state.messages.extend(crate::tui::services::helper_block::welcome_messages(None, state));
                            } else if cmd.command == "/sessions" {
                                let _ = output_tx.try_send(OutputEvent::ListSessions);
                            } else if cmd.command == "/new" {
                                let _ = output_tx.try_send(OutputEvent::NewSession);
                            } else if cmd.command == "/review" {
                                state.add_user_message(cmd.command.clone());
                                state.review_open = true;
                                state.review_generation = state.review_generation.saturating_add(1);
                                state.review_sync_items();
                                state.review_normalize_selection();
                            } else {
                                state.add_user_message(cmd.command.clone());
                                let _ = output_tx.try_send(OutputEvent::UserMessage(cmd.command, None, vec![], None));
                            }
                        }
                        crate::tui::app::CommandSource::BuiltInWithPrompt { prompt_content } |
                        crate::tui::app::CommandSource::Custom { prompt_content } => {
                            state.add_user_message(cmd.command.clone());
                            let _ = output_tx.try_send(OutputEvent::UserMessage(prompt_content, None, vec![], None));
                        }
                    }
                }
                state.show_command_palette = false;
                state.command_palette_input.clear();
                state.command_palette_selected = 0;
            }
            _ => {}
        }
        return;
    }

    // Handle dialog input
    if state.is_dialog_open {
        match event {
            InputEvent::HandleEsc | InputEvent::DialogCancel => {
                if let Some(tc) = state.dialog_command.take() {
                    state.pending_tool_calls.retain(|c| c.id != tc.id);
                    state.rejected_tools.push(tc.clone());
                    let _ = output_tx.try_send(OutputEvent::RejectTool(tc, true));
                }
                state.is_dialog_open = false;
            }
            InputEvent::Up | InputEvent::DialogUp => {
                if state.dialog_selected > 0 {
                    state.dialog_selected -= 1;
                }
            }
            InputEvent::Down | InputEvent::DialogDown => {
                state.dialog_selected = (state.dialog_selected + 1) % 3;
            }
            InputEvent::InputSubmitted | InputEvent::DialogSelect => match state.dialog_selected {
                0 => {
                    if let Some(tc) = state.dialog_command.take() {
                        state.pending_tool_calls.retain(|c| c.id != tc.id);
                        state.approved_tools.push(tc.clone());
                        let _ = output_tx.try_send(OutputEvent::AcceptTool(tc));
                    }
                    state.is_dialog_open = false;
                }
                1 => {
                    if let Some(tc) = state.dialog_command.take() {
                        state.pending_tool_calls.retain(|c| c.id != tc.id);
                        state.rejected_tools.push(tc.clone());
                        let _ = output_tx.try_send(OutputEvent::RejectTool(tc, false));
                    }
                    state.is_dialog_open = false;
                }
                _ => {
                    if let Some(tc) = state.dialog_command.take() {
                        state.pending_tool_calls.retain(|c| c.id != tc.id);
                        state.rejected_tools.push(tc.clone());
                        let _ = output_tx.try_send(OutputEvent::RejectTool(tc, true));
                    }
                    state.is_dialog_open = false;
                }
            },
            InputEvent::ApproveTool => {
                if let Some(tc) = state.dialog_command.take() {
                    state.pending_tool_calls.retain(|c| c.id != tc.id);
                    state.approved_tools.push(tc.clone());
                    let _ = output_tx.try_send(OutputEvent::AcceptTool(tc));
                }
                state.is_dialog_open = false;
            }
            InputEvent::InputChanged('r') | InputEvent::RejectTool => {
                if let Some(tc) = state.dialog_command.take() {
                    state.pending_tool_calls.retain(|c| c.id != tc.id);
                    state.rejected_tools.push(tc.clone());
                    let _ = output_tx.try_send(OutputEvent::RejectTool(tc, false));
                }
                state.is_dialog_open = false;
            }
            _ => {}
        }
        return;
    }

    // Handle shortcuts popup
    if state.show_shortcuts {
        match event {
            InputEvent::HandleEsc | InputEvent::HideShortcuts => {
                state.show_shortcuts = false;
            }
            _ => {}
        }
        return;
    }

    if state.review_open {
        match event {
            InputEvent::HandleEsc | InputEvent::ReviewClose => {
                state.review_open = false;
                state.review_diff = None;
            }
            InputEvent::ReviewOpen => {
                state.review_open = true;
                state.review_generation = state.review_generation.saturating_add(1);
                state.review_sync_items();
                state.review_normalize_selection();
            }
            InputEvent::ReviewUp | InputEvent::Up | InputEvent::ScrollUp => {
                let prev = state.review_selected_path.clone();
                state.review_select_by_delta(-1);
                if state.review_diff.is_some() && prev != state.review_selected_path {
                    if let (Some(path), Ok(session_id)) = (state.review_selected_path.clone(), uuid::Uuid::parse_str(&state.session_id)) {
                        match crate::tui::services::review::load_diff(&state.project_root, session_id, &path) {
                            Ok(diff) => {
                                state.review_diff = Some(crate::tui::app::ReviewDiffState {
                                    path: diff.path,
                                    old_content: Some(diff.old_content),
                                    new_content: Some(diff.new_content),
                                    scroll: 0,
                                    last_error: None,
                                });
                            }
                            Err(e) => {
                                state.review_diff = Some(crate::tui::app::ReviewDiffState {
                                    path,
                                    old_content: None,
                                    new_content: None,
                                    scroll: 0,
                                    last_error: Some(e),
                                });
                            }
                        }
                    }
                }
            }
            InputEvent::ReviewDown | InputEvent::Down | InputEvent::ScrollDown => {
                let prev = state.review_selected_path.clone();
                state.review_select_by_delta(1);
                if state.review_diff.is_some() && prev != state.review_selected_path {
                    if let (Some(path), Ok(session_id)) = (state.review_selected_path.clone(), uuid::Uuid::parse_str(&state.session_id)) {
                        match crate::tui::services::review::load_diff(&state.project_root, session_id, &path) {
                            Ok(diff) => {
                                state.review_diff = Some(crate::tui::app::ReviewDiffState {
                                    path: diff.path,
                                    old_content: Some(diff.old_content),
                                    new_content: Some(diff.new_content),
                                    scroll: 0,
                                    last_error: None,
                                });
                            }
                            Err(e) => {
                                state.review_diff = Some(crate::tui::app::ReviewDiffState {
                                    path,
                                    old_content: None,
                                    new_content: None,
                                    scroll: 0,
                                    last_error: Some(e),
                                });
                            }
                        }
                    }
                }
            }
            InputEvent::ReviewFilterInput(c) | InputEvent::InputChanged(c) => {
                state.review_filter.push(c);
                state.review_normalize_selection();
                if state.review_diff.is_some() {
                    if let (Some(path), Ok(session_id)) = (state.review_selected_path.clone(), uuid::Uuid::parse_str(&state.session_id)) {
                        match crate::tui::services::review::load_diff(&state.project_root, session_id, &path) {
                            Ok(diff) => {
                                state.review_diff = Some(crate::tui::app::ReviewDiffState {
                                    path: diff.path,
                                    old_content: Some(diff.old_content),
                                    new_content: Some(diff.new_content),
                                    scroll: 0,
                                    last_error: None,
                                });
                            }
                            Err(e) => {
                                state.review_diff = Some(crate::tui::app::ReviewDiffState {
                                    path,
                                    old_content: None,
                                    new_content: None,
                                    scroll: 0,
                                    last_error: Some(e),
                                });
                            }
                        }
                    }
                }
            }
            InputEvent::ReviewFilterBackspace | InputEvent::InputBackspace => {
                state.review_filter.pop();
                state.review_normalize_selection();
                if state.review_diff.is_some() {
                    if let (Some(path), Ok(session_id)) = (state.review_selected_path.clone(), uuid::Uuid::parse_str(&state.session_id)) {
                        match crate::tui::services::review::load_diff(&state.project_root, session_id, &path) {
                            Ok(diff) => {
                                state.review_diff = Some(crate::tui::app::ReviewDiffState {
                                    path: diff.path,
                                    old_content: Some(diff.old_content),
                                    new_content: Some(diff.new_content),
                                    scroll: 0,
                                    last_error: None,
                                });
                            }
                            Err(e) => {
                                state.review_diff = Some(crate::tui::app::ReviewDiffState {
                                    path,
                                    old_content: None,
                                    new_content: None,
                                    scroll: 0,
                                    last_error: Some(e),
                                });
                            }
                        }
                    }
                }
            }
            InputEvent::ReviewToggleDiff | InputEvent::InputSubmitted => {
                if let Some(path) = state.review_selected_path.clone() {
                    if state.review_diff.as_ref().map(|d| d.path.as_str()) == Some(path.as_str()) {
                        state.review_diff = None;
                    } else if let Ok(session_id) = uuid::Uuid::parse_str(&state.session_id) {
                        match crate::tui::services::review::load_diff(&state.project_root, session_id, &path) {
                            Ok(diff) => {
                                state.review_diff = Some(crate::tui::app::ReviewDiffState {
                                    path: diff.path,
                                    old_content: Some(diff.old_content),
                                    new_content: Some(diff.new_content),
                                    scroll: 0,
                                    last_error: None,
                                });
                            }
                            Err(e) => {
                                state.review_diff = Some(crate::tui::app::ReviewDiffState {
                                    path,
                                    old_content: None,
                                    new_content: None,
                                    scroll: 0,
                                    last_error: Some(e),
                                });
                            }
                        }
                    }
                }
            }
            InputEvent::PageUp => {
                if let Some(diff) = &mut state.review_diff {
                    let step = 10usize;
                    diff.scroll = diff.scroll.saturating_sub(step);
                }
            }
            InputEvent::PageDown => {
                if let Some(diff) = &mut state.review_diff {
                    diff.scroll = diff.scroll.saturating_add(10);
                    if let (Some(old), Some(new)) = (diff.old_content.as_deref(), diff.new_content.as_deref()) {
                        let total = crate::tui::services::file_diff::render_diff(old, new, 120).len();
                        if total > 0 && diff.scroll >= total {
                            diff.scroll = total - 1;
                        }
                    }
                }
            }
            InputEvent::ReviewRevertSelected | InputEvent::FileChangesRevertFile => {
                let Some(path) = state.review_selected_path.clone() else {
                    return;
                };
                let Ok(session_id) = uuid::Uuid::parse_str(&state.session_id) else {
                    state.add_assistant_message("Invalid session id; cannot restore snapshot.".to_string());
                    return;
                };

                let result = vac_tools::journal::restore_snapshot(&state.project_root, session_id, &path);
                match result {
                    Ok(()) => {
                        state.modified_files.retain(|p| p != &path);
                        state.review_items
                            .entry(path.clone())
                            .and_modify(|it| {
                                it.status = crate::tui::app::ReviewItemStatus::Restored;
                                it.last_error = None;
                                it.dirty_generation = it.dirty_generation.saturating_add(1);
                            });
                        state.add_assistant_message(format!("Reverted file: {}", path));
                    }
                    Err(e) => {
                        state.review_items
                            .entry(path.clone())
                            .and_modify(|it| {
                                it.status = crate::tui::app::ReviewItemStatus::Failed;
                                it.last_error = Some(e.clone());
                                it.dirty_generation = it.dirty_generation.saturating_add(1);
                            });
                        state.add_assistant_message(format!("Failed to revert file: {}", path));
                    }
                }

                state.review_generation = state.review_generation.saturating_add(1);
                state.review_sync_items();
                state.review_normalize_selection();
            }
            InputEvent::ReviewRevertFiltered => {
                let files: Vec<String> = state
                    .review_filtered_paths()
                    .into_iter()
                    .filter(|p| state.modified_files.contains(p))
                    .collect();
                if files.is_empty() {
                    state.add_assistant_message("No files to revert.".to_string());
                    return;
                }
                let Ok(session_id) = uuid::Uuid::parse_str(&state.session_id) else {
                    state.add_assistant_message("Invalid session id; cannot restore snapshot.".to_string());
                    return;
                };

                let mut success_count = 0usize;
                for file in &files {
                    match vac_tools::journal::restore_snapshot(&state.project_root, session_id, file) {
                        Ok(()) => {
                            success_count += 1;
                            state.modified_files.retain(|p| p != file);
                            state.review_items
                                .entry(file.clone())
                                .and_modify(|it| {
                                    it.status = crate::tui::app::ReviewItemStatus::Restored;
                                    it.last_error = None;
                                    it.dirty_generation = it.dirty_generation.saturating_add(1);
                                });
                        }
                        Err(e) => {
                            state.review_items
                                .entry(file.clone())
                                .and_modify(|it| {
                                    it.status = crate::tui::app::ReviewItemStatus::Failed;
                                    it.last_error = Some(e.clone());
                                    it.dirty_generation = it.dirty_generation.saturating_add(1);
                                });
                        }
                    }
                }
                state.add_assistant_message(format!("Reverted {}/{} files.", success_count, files.len()));
                state.review_generation = state.review_generation.saturating_add(1);
                state.review_sync_items();
                state.review_normalize_selection();
            }
            InputEvent::ReviewRevertAll | InputEvent::FileChangesRevertAll => {
                let files = state.modified_files.clone();
                if files.is_empty() {
                    state.add_assistant_message("No files to revert.".to_string());
                    return;
                }
                let Ok(session_id) = uuid::Uuid::parse_str(&state.session_id) else {
                    state.add_assistant_message("Invalid session id; cannot restore snapshot.".to_string());
                    return;
                };

                let mut success_count = 0usize;
                for file in &files {
                    if vac_tools::journal::restore_snapshot(&state.project_root, session_id, file).is_ok() {
                        success_count += 1;
                        state.review_items
                            .entry(file.clone())
                            .and_modify(|it| {
                                it.status = crate::tui::app::ReviewItemStatus::Restored;
                                it.last_error = None;
                                it.dirty_generation = it.dirty_generation.saturating_add(1);
                            });
                    } else {
                        state.review_items
                            .entry(file.clone())
                            .and_modify(|it| {
                                it.status = crate::tui::app::ReviewItemStatus::Failed;
                                it.dirty_generation = it.dirty_generation.saturating_add(1);
                            });
                    }
                }
                state.modified_files.clear();
                state.review_diff = None;
                state.review_selected_idx = 0;
                state.review_selected_path = None;
                state.add_assistant_message(format!("Reverted {}/{} files.", success_count, files.len()));
                state.review_generation = state.review_generation.saturating_add(1);
                state.review_sync_items();
                state.review_normalize_selection();
            }
            InputEvent::ReviewOpenEditor | InputEvent::FileChangesOpenEditor => {
                let Some(path) = state.review_selected_path.clone() else {
                    return;
                };
                let preferred = std::env::var("VAC_EDITOR")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| std::env::var("EDITOR").ok().filter(|s| !s.trim().is_empty()))
                    .and_then(|s| s.split_whitespace().next().map(|t| t.to_string()));

                let Some(editor) = crate::tui::services::review::detect_editor(preferred) else {
                    state.add_assistant_message("No editor available. Set VAC_EDITOR/EDITOR or install nvim/vim/nano.".to_string());
                    return;
                };

                let _ = disable_raw_mode();
                let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
                let _ = Command::new(editor).arg(&path).status();
                let _ = execute!(std::io::stdout(), EnterAlternateScreen, EnableBracketedPaste, EnableMouseCapture, Clear(ClearType::All));
                let _ = enable_raw_mode();
            }
            _ => {}
        }
        return;
    }

    // Handle File Changes Popup
    if state.show_file_changes_popup {
        match event {
            InputEvent::HandleEsc => {
                state.show_file_changes_popup = false;
            }
            InputEvent::Up | InputEvent::ScrollUp => {
                if state.file_changes_selected > 0 {
                    state.file_changes_selected -= 1;
                }
            }
            InputEvent::Down | InputEvent::ScrollDown => {
                if state.file_changes_selected < state.modified_files.len().saturating_sub(1) {
                    state.file_changes_selected += 1;
                }
            }
            InputEvent::InputChanged(c) => {
                state.file_changes_search.push(c);
                state.file_changes_selected = 0;
            }
            InputEvent::InputBackspace => {
                state.file_changes_search.pop();
                state.file_changes_selected = 0;
            }
            InputEvent::FileChangesRevertFile => {
                if let Some(file) = state.modified_files.get(state.file_changes_selected).cloned() {
                    let mut success = false;
                    if let Ok(session_id) = uuid::Uuid::parse_str(&state.session_id) {
                        if vac_tools::journal::restore_snapshot(&state.project_root, session_id, &file).is_ok() {
                            success = true;
                        }
                    }
                    
                    if success {
                        state.add_assistant_message(format!("Reverted file: {}", file));
                        state.modified_files.retain(|f| f != &file);
                        if state.file_changes_selected >= state.modified_files.len() {
                            state.file_changes_selected = state.modified_files.len().saturating_sub(1);
                        }
                        if state.modified_files.is_empty() {
                            state.show_file_changes_popup = false;
                        }
                    } else {
                        state.add_assistant_message(format!("Failed to revert file: {}", file));
                    }
                }
            }
            InputEvent::FileChangesRevertAll => {
                let files = state.modified_files.clone();
                if files.is_empty() {
                    state.add_assistant_message("No files to revert.".to_string());
                } else {
                    let mut success_count = 0;
                    if let Ok(session_id) = uuid::Uuid::parse_str(&state.session_id) {
                        for file in &files {
                            if vac_tools::journal::restore_snapshot(&state.project_root, session_id, file).is_ok() {
                                success_count += 1;
                            }
                        }
                    }
                    state.add_assistant_message(format!("Reverted {}/{} files.", success_count, files.len()));
                    state.modified_files.clear();
                    state.file_changes_selected = 0;
                    state.show_file_changes_popup = false;
                }
            }
            InputEvent::FileChangesOpenEditor => {
                if let Some(file) = state.modified_files.get(state.file_changes_selected).cloned() {
                    state.add_assistant_message(format!("Opening editor for: {}", file));
                }
            }
            _ => {}
        }
        return;
    }

    // Normal input handling
    match event {
        InputEvent::InputChanged(c) => {
            state.input.input(c);
        }
        InputEvent::InputChangedNewline => {
            state.input.newline();
        }
        InputEvent::InputBackspace => {
            state.input.backspace();
        }
        InputEvent::InputDelete => {
            state.input.delete();
        }
        InputEvent::InputClear => {
            state.input.clear();
        }
        InputEvent::InputSubmitted => {
            if !state.input.is_empty() {
                let msg = state.input.get_content();
                state.input.clear();
                
                if msg.starts_with('/') {
                    let trimmed = msg.trim();
                    let mut parts = trimmed.splitn(2, char::is_whitespace);
                    let cmd_word = parts.next().unwrap_or(trimmed);
                    let cmd_args = parts.next().map(|s| s.trim()).filter(|s| !s.is_empty());

                    if let Some(cmd) = state.commands.iter().find(|c| c.command == cmd_word).cloned() {
                        match cmd.source {
                            crate::tui::app::CommandSource::BuiltIn => {
                                if cmd.command == "/clear" {
                                    state.messages.clear();
                                    state.messages.extend(crate::tui::services::helper_block::welcome_messages(None, state));
                                } else if cmd.command == "/sessions" {
                                    let _ = output_tx.try_send(OutputEvent::ListSessions);
                                } else if cmd.command == "/new" {
                                    let _ = output_tx.try_send(OutputEvent::NewSession);
                                } else if cmd.command == "/review" {
                                    state.add_user_message(trimmed.to_string());
                                    state.review_open = true;
                                    state.review_generation = state.review_generation.saturating_add(1);
                                    state.review_sync_items();
                                    state.review_normalize_selection();
                                } else {
                                    state.add_user_message(trimmed.to_string());
                                    let _ = output_tx.try_send(OutputEvent::UserMessage(trimmed.to_string(), None, vec![], None));
                                }
                            }
                            crate::tui::app::CommandSource::BuiltInWithPrompt { prompt_content } |
                            crate::tui::app::CommandSource::Custom { prompt_content } => {
                                state.add_user_message(trimmed.to_string());
                                let prompt = match cmd_args {
                                    Some(args) => format!("{prompt_content}\n\n{args}"),
                                    None => prompt_content,
                                };
                                let _ = output_tx.try_send(OutputEvent::UserMessage(prompt, None, vec![], None));
                            }
                        }
                    } else {
                        state.add_user_message(msg.clone());
                        let _ = output_tx.try_send(OutputEvent::UserMessage(msg, None, vec![], None));
                    }
                } else {
                    state.add_user_message(msg.clone());
                    let _ = output_tx.try_send(OutputEvent::UserMessage(msg, None, vec![], None));
                }
            }
        }
        InputEvent::HandlePaste(text) => {
            for c in text.chars() {
                if c == '\n' {
                    state.input.newline();
                } else if c != '\r' {
                    state.input.input(c);
                }
            }
        }
        InputEvent::CursorLeft => {
            state.input.move_cursor_left();
        }
        InputEvent::CursorRight => {
            state.input.move_cursor_right();
        }
        InputEvent::Up => {
            state.input.move_cursor_up();
        }
        InputEvent::Down => {
            state.input.move_cursor_down();
        }
        InputEvent::InputCursorStart => {
            state.input.move_cursor_start();
        }
        InputEvent::InputCursorEnd => {
            state.input.move_cursor_end();
        }
        InputEvent::AttemptQuit => {
            state.cancel_requested = true;
        }
        InputEvent::HandleEsc => {
            if state.is_streaming {
                let _ = output_tx.try_send(OutputEvent::CancelStream);
                state.is_streaming = false;
            }
        }
        InputEvent::ScrollUp => {
            state.scroll = state.scroll.saturating_sub(1);
        }
        InputEvent::ScrollDown => {
            state.scroll = state.scroll.saturating_add(1);
        }
        InputEvent::ShowCommandPalette => {
            state.show_command_palette = true;
            state.command_palette_input.clear();
            state.command_palette_selected = 0;
        }
        InputEvent::ShowShortcuts => {
            state.show_shortcuts = true;
        }
        InputEvent::ShowFileChangesPopup => {
            state.show_file_changes_popup = true;
            state.file_changes_selected = 0;
            state.file_changes_search.clear();
        }
        InputEvent::ToggleAutoApprove => {
            state.auto_approve = !state.auto_approve;
            if state.auto_approve {
                state.add_assistant_message("Permission Mode: AUTO-APPROVE (Low-risk tools will run without confirmation)".to_string());
            } else {
                state.add_assistant_message("Permission Mode: PROMPT (You will be prompted for tool execution)".to_string());
            }
        }
        InputEvent::RequestSessionList => {
            let _ = output_tx.try_send(OutputEvent::ListSessions);
        }
        InputEvent::NewSession => {
            let _ = output_tx.try_send(OutputEvent::NewSession);
        }
        _ => {}
    }
}

/// Handle backend events
fn is_low_risk_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "file_read" | "file_write" | "file_edit" | "glob" | "grep" | "search"
            | "vil_knowledge" | "vil_diagnostics" | "vil_status" | "vil_lsp_query"
    )
}

fn handle_backend_event(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    match event {
        InputEvent::AssistantMessage(msg) => {
            state.add_assistant_message(msg);
            state.loading = false;
        }
        InputEvent::StreamAssistantMessage(id, chunk) => {
            state.is_streaming = true;
            state.streaming_message_id = Some(id);
            if let Some(last) = state.messages.last_mut() {
                if last.role == "assistant" {
                    last.content.push_str(&chunk);
                    return;
                }
            }
            state.add_assistant_message(chunk);
        }
        InputEvent::StartLoadingOperation(op) => {
            state.loading_manager.start_operation(op);
            state.loading = true;
        }
        InputEvent::EndLoadingOperation(op) => {
            state.loading_manager.end_operation(op);
            state.loading = state.loading_manager.is_loading();
            state.is_streaming = false;
        }
        InputEvent::Error(msg) => {
            state.add_assistant_message(format!("Error: {}", msg));
            state.loading = false;
            state.is_streaming = false;
        }
        InputEvent::SetSessions(sessions) => {
            state.sessions = sessions;
        }
        InputEvent::SessionRestored { id, title, messages } => {
            state.session_id = id;
            state.session_title = Some(title);
            state.messages = messages;
            state.loading = false;
            
            // Clear transient UI state to prevent leakage between sessions
            state.pending_tool_calls.clear();
            state.approved_tools.clear();
            state.rejected_tools.clear();
            state.dialog_command = None;
            state.is_dialog_open = false;
            state.is_streaming = false;
            state.streaming_message_id = None;
            state.scroll = 0;
            state.input.clear();
        }
        InputEvent::ShowConfirmationDialog(tc) => {
            state.dialog_command = Some(tc);
            state.is_dialog_open = true;
            state.dialog_selected = 0;
            state.permission_explanation = None;
        }
        InputEvent::ShowConfirmationDialogWithExplanation(tc, explanation) => {
            if state.auto_approve && is_low_risk_tool(&tc.function.name) {
                state.approved_tools.push(tc.clone());
                let _ = output_tx.try_send(OutputEvent::AcceptTool(tc));
                state.is_dialog_open = false;
                state.dialog_command = None;
                state.permission_explanation = None;
            } else {
                state.dialog_command = Some(tc);
                state.is_dialog_open = true;
                state.dialog_selected = 0;
                state.permission_explanation = explanation;
            }
        }
        InputEvent::RunToolCall(tc) => {
            state.pending_tool_calls.push(tc);
        }
        InputEvent::ToolResult(result) => {
            state.pending_tool_calls.retain(|c| c.id != result.call.id);
            state.approved_tools.retain(|c| c.id != result.call.id);
            state.add_assistant_message(result.result);
        }
        InputEvent::TaskCompleted(result) => {
            let mut content = result.summary.clone();
            if !result.modified_files.is_empty() {
                content.push_str("\n\n**Modified Files**:\n");
                for file in &result.modified_files {
                    content.push_str(&format!("- `{}`\n", file));
                    if !state.modified_files.contains(file) {
                        state.modified_files.push(file.clone());
                    }
                }
            }
            if !result.created_files.is_empty() {
                content.push_str("\n**Created Files**:\n");
                for file in &result.created_files {
                    content.push_str(&format!("- `{}`\n", file));
                    if !state.modified_files.contains(file) {
                        state.modified_files.push(file.clone());
                    }
                }
            }
            if state.review_open {
                state.review_generation = state.review_generation.saturating_add(1);
                state.review_sync_items();
                state.review_normalize_selection();
            }
            state.add_assistant_message(content);
            state.loading = false;
            state.is_streaming = false;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(project_root: std::path::PathBuf, session_id: uuid::Uuid) -> AppState {
        AppState::new(AppStateOptions {
            model: None,
            session_id: Some(session_id.to_string()),
            checkpoint_path: Some(project_root.join(".vac/checkpoints")),
            project_root,
        })
    }

    #[test]
    fn review_selection_normalizes_when_filter_excludes_selected() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.modified_files = vec!["a.txt".to_string(), "b.txt".to_string()];
        state.review_open = true;
        state.review_selected_path = Some("b.txt".to_string());
        state.review_filter = "a".to_string();
        state.review_sync_items();
        state.review_normalize_selection();
        assert_eq!(state.review_selected_path, Some("a.txt".to_string()));
        assert_eq!(state.review_selected_idx, 0);
    }

    #[tokio::test]
    async fn slash_semantics_fix_and_explain_send_prompt_with_args() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        state.input.set_content("/fix cargo clippy");
        handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        let ev = rx.recv().await.unwrap();
        match ev {
            OutputEvent::UserMessage(prompt, _, _, _) => {
                assert!(prompt.contains("Fix linter/build errors"));
                assert!(prompt.contains("cargo clippy"));
                assert!(!prompt.trim_start().starts_with("/fix"));
            }
            _ => panic!("unexpected event"),
        }

        state.input.set_content("/explain crates/vac_cli/src/tui/event_loop.rs");
        handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        let ev = rx.recv().await.unwrap();
        match ev {
            OutputEvent::UserMessage(prompt, _, _, _) => {
                assert!(prompt.contains("Explain the relevant code or concept"));
                assert!(prompt.contains("crates/vac_cli/src/tui/event_loop.rs"));
                assert!(!prompt.trim_start().starts_with("/explain"));
            }
            _ => panic!("unexpected event"),
        }
    }

    #[test]
    fn slash_review_opens_workstation_instead_of_sending_literal() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.modified_files = vec!["a.txt".to_string()];
        state.input.set_content("/review");
        handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert!(state.review_open);
    }

    #[test]
    fn review_open_close_transitions_clear_diff() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.review_open = true;
        state.review_diff = Some(crate::tui::app::ReviewDiffState {
            path: "a.txt".to_string(),
            old_content: Some("old".to_string()),
            new_content: Some("new".to_string()),
            scroll: 0,
            last_error: None,
        });
        handle_input_event(&mut state, &tx, InputEvent::ReviewClose);
        assert!(!state.review_open);
        assert!(state.review_diff.is_none());
    }

    #[test]
    fn diff_scroll_state_changes_on_page_down() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.review_open = true;
        state.review_diff = Some(crate::tui::app::ReviewDiffState {
            path: "a.txt".to_string(),
            old_content: Some("a\nb\nc\nd\ne\n".to_string()),
            new_content: Some("a\nb\nX\nd\ne\n".to_string()),
            scroll: 0,
            last_error: None,
        });
        handle_input_event(&mut state, &tx, InputEvent::PageDown);
        assert!(state.review_diff.as_ref().unwrap().scroll > 0);
    }

    #[tokio::test]
    async fn revert_selected_updates_status_and_working_tree() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let session_id = uuid::Uuid::new_v4();
        std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

        let file_rel = "a.txt";
        std::fs::write(root.join(file_rel), "new").unwrap();
        std::fs::write(crate::tui::services::review::snapshot_path(&root, session_id, file_rel), "old").unwrap();

        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(root.clone(), session_id);
        state.modified_files = vec![file_rel.to_string()];
        state.review_open = true;
        state.review_sync_items();
        state.review_selected_path = Some(file_rel.to_string());

        handle_input_event(&mut state, &tx, InputEvent::ReviewRevertSelected);

        let content = std::fs::read_to_string(root.join(file_rel)).unwrap();
        assert_eq!(content, "old");
        assert!(!state.modified_files.contains(&file_rel.to_string()));
        let it = state.review_items.get(file_rel).unwrap();
        assert_eq!(it.status, crate::tui::app::ReviewItemStatus::Restored);
    }

    #[tokio::test]
    async fn revert_filtered_only_affects_filtered_modified_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let session_id = uuid::Uuid::new_v4();
        std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

        for (p, old, new) in [
            ("a.txt", "old-a", "new-a"),
            ("b.txt", "old-b", "new-b"),
        ] {
            std::fs::write(root.join(p), new).unwrap();
            std::fs::write(crate::tui::services::review::snapshot_path(&root, session_id, p), old).unwrap();
        }

        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(root.clone(), session_id);
        state.modified_files = vec!["a.txt".to_string(), "b.txt".to_string()];
        state.review_open = true;
        state.review_filter = "a".to_string();
        state.review_sync_items();
        state.review_normalize_selection();

        handle_input_event(&mut state, &tx, InputEvent::ReviewRevertFiltered);

        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "old-a");
        assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "new-b");
        assert!(!state.modified_files.contains(&"a.txt".to_string()));
        assert!(state.modified_files.contains(&"b.txt".to_string()));
    }

    #[tokio::test]
    async fn revert_all_clears_modified_files_and_marks_status() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let session_id = uuid::Uuid::new_v4();
        std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

        for (p, old, new) in [
            ("a.txt", "old-a", "new-a"),
            ("b.txt", "old-b", "new-b"),
        ] {
            std::fs::write(root.join(p), new).unwrap();
            std::fs::write(crate::tui::services::review::snapshot_path(&root, session_id, p), old).unwrap();
        }

        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(root.clone(), session_id);
        state.modified_files = vec!["a.txt".to_string(), "b.txt".to_string()];
        state.review_open = true;
        state.review_sync_items();
        state.review_normalize_selection();

        handle_input_event(&mut state, &tx, InputEvent::ReviewRevertAll);

        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "old-a");
        assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "old-b");
        assert!(state.modified_files.is_empty());
        assert_eq!(
            state.review_items.get("a.txt").unwrap().status,
            crate::tui::app::ReviewItemStatus::Restored
        );
        assert_eq!(
            state.review_items.get("b.txt").unwrap().status,
            crate::tui::app::ReviewItemStatus::Restored
        );
    }
}
