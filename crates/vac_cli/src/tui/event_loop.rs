//! Event Loop Module

use crate::tui::Model;
use crate::tui::app::{AppState, AppStateOptions, InputEvent, OutputEvent};
use crate::tui::event::map_crossterm_event_to_input_event;
use crate::tui::services::helper_block::welcome_messages;
use crate::tui::terminal::TerminalGuard;
use crate::tui::view::view;
use crossterm::{
    event::{EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    let checkpoint_path = std::env::var("VAC_CHECKPOINT")
        .ok()
        .map(std::path::PathBuf::from);

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
                if let Some(event) = crossterm::event::read()
                    .ok()
                    .and_then(map_crossterm_event_to_input_event)
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

        state.toasts.retain(|t| !t.is_expired());
        if state.toasts.len() > 3 {
            state.toasts.drain(0..state.toasts.len().saturating_sub(3));
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
fn handle_input_event(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
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
                                state.messages.extend(
                                    crate::tui::services::helper_block::welcome_messages(
                                        None, state,
                                    ),
                                );
                            } else if cmd.command == "/sessions" {
                                state.workbench_tab = crate::tui::app::WorkbenchTab::Sessions;
                                state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                                let _ = output_tx.try_send(OutputEvent::ListSessions);
                            } else if cmd.command == "/new" {
                                let _ = output_tx.try_send(OutputEvent::NewSession);
                            } else if cmd.command == "/review" {
                                state.add_user_message(cmd.command.clone());
                                state.review_open = true;
                                state.workbench_tab = crate::tui::app::WorkbenchTab::Review;
                                state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                                state.push_activity(crate::tui::app::ActivityKind::Review, "Open review");
                                state.review_generation = state.review_generation.saturating_add(1);
                                state.review_sync_items();
                                state.review_normalize_selection();
                            } else if cmd.command == "/model" {
                                state.add_user_message(cmd.command.clone());
                                state.show_model_switcher = true;
                                state.model_switcher_filter.clear();
                                state.model_switcher_selected_idx = 0;
                            } else if cmd.command == "/files" {
                                state.add_user_message(cmd.command.clone());
                                state.show_file_search = true;
                                state.file_search_query.clear();
                                state.file_search_selected_idx = 0;
                                if state.all_files.is_empty() {
                                    state.all_files =
                                        crate::tui::services::build_file_index(&state.project_root);
                                }
                                state.file_search_results = crate::tui::services::fuzzy_search_files(
                                    "",
                                    &state.all_files,
                                    50,
                                );
                            } else if cmd.command == "/changes" {
                                state.add_user_message(cmd.command.clone());
                                state.show_changeset = true;
                                state.changeset_selected_idx = 0;
                                state.changeset_diff_scroll = 0;
                                state.changeset_selected_path =
                                    state.modified_files.first().cloned();
                                state.changeset_diff = state.changeset_selected_path.clone().map(|p| {
                                    let session_id = uuid::Uuid::parse_str(&state.session_id).ok();
                                    if let Some(session_id) = session_id {
                                        match crate::tui::services::review::load_diff(
                                            &state.project_root,
                                            session_id,
                                            &p,
                                        ) {
                                            Ok(diff) => crate::tui::app::ReviewDiffState {
                                                path: diff.path,
                                                old_content: Some(diff.old_content),
                                                new_content: Some(diff.new_content),
                                                scroll: 0,
                                                last_error: None,
                                            },
                                            Err(e) => crate::tui::app::ReviewDiffState {
                                                path: p,
                                                old_content: None,
                                                new_content: None,
                                                scroll: 0,
                                                last_error: Some(e),
                                            },
                                        }
                                    } else {
                                        crate::tui::app::ReviewDiffState {
                                            path: p,
                                            old_content: None,
                                            new_content: None,
                                            scroll: 0,
                                            last_error: Some("Invalid session id".to_string()),
                                        }
                                    }
                                });
                            } else {
                                state.add_user_message(cmd.command.clone());
                                let _ = output_tx.try_send(OutputEvent::UserMessage(
                                    cmd.command,
                                    None,
                                    vec![],
                                    None,
                                ));
                            }
                        }
                        crate::tui::app::CommandSource::BuiltInWithPrompt { prompt_content }
                        | crate::tui::app::CommandSource::Custom { prompt_content } => {
                            state.add_user_message(cmd.command.clone());
                            let _ = output_tx.try_send(OutputEvent::UserMessage(
                                prompt_content,
                                None,
                                vec![],
                                None,
                            ));
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

    if state.show_model_switcher {
        match event {
            InputEvent::HandleEsc => {
                state.show_model_switcher = false;
                state.model_switcher_filter.clear();
                state.model_switcher_selected_idx = 0;
            }
            InputEvent::InputChanged(c) => {
                state.model_switcher_filter.push(c);
                state.model_switcher_selected_idx = 0;
            }
            InputEvent::InputBackspace => {
                state.model_switcher_filter.pop();
                state.model_switcher_selected_idx = 0;
            }
            InputEvent::Up => {
                let len = state.model_switcher_filtered().len();
                if len > 0 {
                    state.model_switcher_selected_idx =
                        state.model_switcher_selected_idx.saturating_sub(1).min(len - 1);
                }
            }
            InputEvent::Down => {
                let len = state.model_switcher_filtered().len();
                if len > 0 {
                    state.model_switcher_selected_idx =
                        (state.model_switcher_selected_idx + 1).min(len - 1);
                }
            }
            InputEvent::InputSubmitted => {
                let models = state.model_switcher_filtered();
                if let Some(selected) = models.get(state.model_switcher_selected_idx).cloned() {
                    state.current_model = Some(selected.clone());
                    let _ = output_tx.try_send(OutputEvent::SwitchToModel(selected.clone()));
                    state.show_model_switcher = false;
                    state.model_switcher_filter.clear();
                    state.model_switcher_selected_idx = 0;
                    state.push_activity(
                        crate::tui::app::ActivityKind::Status,
                        format!("Model switched: {}", selected.name),
                    );
                }
            }
            _ => {}
        }
        return;
    }

    if state.show_file_search {
        if state.all_files.is_empty() {
            state.all_files = crate::tui::services::build_file_index(&state.project_root);
        }
        if state.file_search_results.is_empty() {
            state.file_search_results = crate::tui::services::fuzzy_search_files(
                &state.file_search_query,
                &state.all_files,
                50,
            );
            state.file_search_selected_idx = state
                .file_search_selected_idx
                .min(state.file_search_results.len().saturating_sub(1));
        }

        match event {
            InputEvent::HandleEsc => {
                state.show_file_search = false;
                state.file_search_query.clear();
                state.file_search_selected_idx = 0;
                state.file_search_results.clear();
            }
            InputEvent::InputChanged(c) => {
                state.file_search_query.push(c);
                state.file_search_selected_idx = 0;
                state.file_search_results = crate::tui::services::fuzzy_search_files(
                    &state.file_search_query,
                    &state.all_files,
                    50,
                );
            }
            InputEvent::InputBackspace => {
                state.file_search_query.pop();
                state.file_search_selected_idx = 0;
                state.file_search_results = crate::tui::services::fuzzy_search_files(
                    &state.file_search_query,
                    &state.all_files,
                    50,
                );
            }
            InputEvent::Up => {
                if !state.file_search_results.is_empty() {
                    state.file_search_selected_idx =
                        state.file_search_selected_idx.saturating_sub(1);
                }
            }
            InputEvent::Down => {
                if !state.file_search_results.is_empty() {
                    state.file_search_selected_idx = (state.file_search_selected_idx + 1)
                        .min(state.file_search_results.len().saturating_sub(1));
                }
            }
            InputEvent::InputSubmitted => {
                if let Some(path) = state
                    .file_search_results
                    .get(state.file_search_selected_idx)
                    .cloned()
                {
                    state.show_file_search = false;
                    state.file_search_query.clear();
                    state.file_search_selected_idx = 0;
                    state.file_search_results.clear();
                    state.focus = crate::tui::app::WorkspaceFocus::Input;
                    state.input.insert_str(&path);
                    state.toasts
                        .push(crate::tui::services::Toast::info(format!("Inserted: {}", path)));
                }
            }
            _ => {}
        }
        return;
    }

    if state.show_changeset {
        let load_diff = |state: &AppState, path: &str| -> crate::tui::app::ReviewDiffState {
            let session_id = uuid::Uuid::parse_str(&state.session_id).ok();
            if let Some(session_id) = session_id {
                match crate::tui::services::review::load_diff(&state.project_root, session_id, path)
                {
                    Ok(diff) => crate::tui::app::ReviewDiffState {
                        path: diff.path,
                        old_content: Some(diff.old_content),
                        new_content: Some(diff.new_content),
                        scroll: 0,
                        last_error: None,
                    },
                    Err(e) => crate::tui::app::ReviewDiffState {
                        path: path.to_string(),
                        old_content: None,
                        new_content: None,
                        scroll: 0,
                        last_error: Some(e),
                    },
                }
            } else {
                crate::tui::app::ReviewDiffState {
                    path: path.to_string(),
                    old_content: None,
                    new_content: None,
                    scroll: 0,
                    last_error: Some("Invalid session id".to_string()),
                }
            }
        };

        if state.changeset_selected_path.is_none() {
            state.changeset_selected_idx = state
                .changeset_selected_idx
                .min(state.modified_files.len().saturating_sub(1));
            state.changeset_selected_path = state
                .modified_files
                .get(state.changeset_selected_idx)
                .cloned();
            if let Some(path) = state.changeset_selected_path.clone() {
                state.changeset_diff = Some(load_diff(state, &path));
            }
        }

        match event {
            InputEvent::HandleEsc => {
                state.show_changeset = false;
                state.changeset_selected_idx = 0;
                state.changeset_diff_scroll = 0;
                state.changeset_selected_path = None;
                state.changeset_diff = None;
            }
            InputEvent::Up => {
                if !state.modified_files.is_empty() {
                    state.changeset_selected_idx =
                        state.changeset_selected_idx.saturating_sub(1);
                    state.changeset_diff_scroll = 0;
                    state.changeset_selected_path = state
                        .modified_files
                        .get(state.changeset_selected_idx)
                        .cloned();
                    if let Some(path) = state.changeset_selected_path.clone() {
                        state.changeset_diff = Some(load_diff(state, &path));
                    }
                }
            }
            InputEvent::Down => {
                if !state.modified_files.is_empty() {
                    state.changeset_selected_idx = (state.changeset_selected_idx + 1)
                        .min(state.modified_files.len().saturating_sub(1));
                    state.changeset_diff_scroll = 0;
                    state.changeset_selected_path = state
                        .modified_files
                        .get(state.changeset_selected_idx)
                        .cloned();
                    if let Some(path) = state.changeset_selected_path.clone() {
                        state.changeset_diff = Some(load_diff(state, &path));
                    }
                }
            }
            InputEvent::ScrollUp => {
                state.changeset_diff_scroll = state.changeset_diff_scroll.saturating_sub(1);
            }
            InputEvent::ScrollDown => {
                state.changeset_diff_scroll = state.changeset_diff_scroll.saturating_add(1);
            }
            _ => {}
        }
        return;
    }

    if state.review_open
        && state.focus == crate::tui::app::WorkspaceFocus::Workbench
        && state.workbench_tab == crate::tui::app::WorkbenchTab::Review
    {
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
                    if let (Some(path), Ok(session_id)) = (
                        state.review_selected_path.clone(),
                        uuid::Uuid::parse_str(&state.session_id),
                    ) {
                        match crate::tui::services::review::load_diff(
                            &state.project_root,
                            session_id,
                            &path,
                        ) {
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
                    if let (Some(path), Ok(session_id)) = (
                        state.review_selected_path.clone(),
                        uuid::Uuid::parse_str(&state.session_id),
                    ) {
                        match crate::tui::services::review::load_diff(
                            &state.project_root,
                            session_id,
                            &path,
                        ) {
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
                    if let (Some(path), Ok(session_id)) = (
                        state.review_selected_path.clone(),
                        uuid::Uuid::parse_str(&state.session_id),
                    ) {
                        match crate::tui::services::review::load_diff(
                            &state.project_root,
                            session_id,
                            &path,
                        ) {
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
                    if let (Some(path), Ok(session_id)) = (
                        state.review_selected_path.clone(),
                        uuid::Uuid::parse_str(&state.session_id),
                    ) {
                        match crate::tui::services::review::load_diff(
                            &state.project_root,
                            session_id,
                            &path,
                        ) {
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
                        match crate::tui::services::review::load_diff(
                            &state.project_root,
                            session_id,
                            &path,
                        ) {
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
                    if let (Some(old), Some(new)) =
                        (diff.old_content.as_deref(), diff.new_content.as_deref())
                    {
                        let total =
                            crate::tui::services::file_diff::render_diff(old, new, 120).len();
                        if total > 0 && diff.scroll >= total {
                            diff.scroll = total - 1;
                        }
                    }
                }
            }
            InputEvent::ReviewRevertSelected => {
                let Some(path) = state.review_selected_path.clone() else {
                    return;
                };
                let Ok(session_id) = uuid::Uuid::parse_str(&state.session_id) else {
                    state.add_assistant_message(
                        "Invalid session id; cannot restore snapshot.".to_string(),
                    );
                    return;
                };

                let result =
                    vac_tools::journal::restore_snapshot(&state.project_root, session_id, &path);
                match result {
                    Ok(()) => {
                        state.changeset_store.revert_success(&path);
                        state.modified_files.retain(|p| p != &path);
                        state.review_items.entry(path.clone()).and_modify(|it| {
                            it.status = crate::tui::app::ReviewItemStatus::Restored;
                            it.last_error = None;
                            it.dirty_generation = it.dirty_generation.saturating_add(1);
                        });
                        state.add_assistant_message(format!("Reverted file: {}", path));
                        state.push_activity(
                            crate::tui::app::ActivityKind::Review,
                            format!("Reverted: {path}"),
                        );
                    }
                    Err(e) => {
                        state.changeset_store.revert_failed(&path, e.clone());
                        state.review_items.entry(path.clone()).and_modify(|it| {
                            it.status = crate::tui::app::ReviewItemStatus::Failed;
                            it.last_error = Some(e.clone());
                            it.dirty_generation = it.dirty_generation.saturating_add(1);
                        });
                        state.add_assistant_message(format!("Failed to revert file: {}", path));
                        state.push_activity(
                            crate::tui::app::ActivityKind::Review,
                            format!("Revert failed: {path}"),
                        );
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
                    state.add_assistant_message(
                        "Invalid session id; cannot restore snapshot.".to_string(),
                    );
                    return;
                };

                let mut success_count = 0usize;
                for file in &files {
                    match vac_tools::journal::restore_snapshot(
                        &state.project_root,
                        session_id,
                        file,
                    ) {
                        Ok(()) => {
                            success_count += 1;
                            state.changeset_store.revert_success(file);
                            state.modified_files.retain(|p| p != file);
                            state.review_items.entry(file.clone()).and_modify(|it| {
                                it.status = crate::tui::app::ReviewItemStatus::Restored;
                                it.last_error = None;
                                it.dirty_generation = it.dirty_generation.saturating_add(1);
                            });
                        }
                        Err(e) => {
                            state.changeset_store.revert_failed(file, e.clone());
                            state.review_items.entry(file.clone()).and_modify(|it| {
                                it.status = crate::tui::app::ReviewItemStatus::Failed;
                                it.last_error = Some(e.clone());
                                it.dirty_generation = it.dirty_generation.saturating_add(1);
                            });
                        }
                    }
                }
                state.add_assistant_message(format!(
                    "Reverted {}/{} files.",
                    success_count,
                    files.len()
                ));
                state.push_activity(
                    crate::tui::app::ActivityKind::Review,
                    format!("Reverted filtered: {success_count}/{}", files.len()),
                );
                state.review_generation = state.review_generation.saturating_add(1);
                state.review_sync_items();
                state.review_normalize_selection();
            }
            InputEvent::ReviewRevertAll => {
                let files = state.modified_files.clone();
                if files.is_empty() {
                    state.add_assistant_message("No files to revert.".to_string());
                    return;
                }
                let Ok(session_id) = uuid::Uuid::parse_str(&state.session_id) else {
                    state.add_assistant_message(
                        "Invalid session id; cannot restore snapshot.".to_string(),
                    );
                    return;
                };

                let mut success_count = 0usize;
                for file in &files {
                    match vac_tools::journal::restore_snapshot(&state.project_root, session_id, file) {
                        Ok(()) => {
                            success_count += 1;
                            state.changeset_store.revert_success(file);
                            state.review_items.entry(file.clone()).and_modify(|it| {
                                it.status = crate::tui::app::ReviewItemStatus::Restored;
                                it.last_error = None;
                                it.dirty_generation = it.dirty_generation.saturating_add(1);
                            });
                        }
                        Err(e) => {
                            state.changeset_store.revert_failed(file, e.clone());
                            state.review_items.entry(file.clone()).and_modify(|it| {
                                it.status = crate::tui::app::ReviewItemStatus::Failed;
                                it.last_error = Some(e);
                                it.dirty_generation = it.dirty_generation.saturating_add(1);
                            });
                        }
                    }
                }
                state.modified_files.clear();
                state.review_diff = None;
                state.review_selected_idx = 0;
                state.review_selected_path = None;
                state.add_assistant_message(format!(
                    "Reverted {}/{} files.",
                    success_count,
                    files.len()
                ));
                state.push_activity(
                    crate::tui::app::ActivityKind::Review,
                    format!("Reverted all: {success_count}/{}", files.len()),
                );
                state.review_generation = state.review_generation.saturating_add(1);
                state.review_sync_items();
                state.review_normalize_selection();
            }
            InputEvent::ReviewOpenEditor => {
                let Some(path) = state.review_selected_path.clone() else {
                    return;
                };
                let preferred = std::env::var("VAC_EDITOR")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| {
                        std::env::var("EDITOR")
                            .ok()
                            .filter(|s| !s.trim().is_empty())
                    })
                    .and_then(|s| s.split_whitespace().next().map(|t| t.to_string()));

                let Some(editor) = crate::tui::services::review::detect_editor(preferred) else {
                    state.add_assistant_message(
                        "No editor available. Set VAC_EDITOR/EDITOR or install nvim/vim/nano."
                            .to_string(),
                    );
                    return;
                };

                state.push_activity(
                    crate::tui::app::ActivityKind::Review,
                    format!("Open editor: {path}"),
                );

                let _ = disable_raw_mode();
                let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
                let _ = Command::new(editor).arg(&path).status();
                let _ = execute!(
                    std::io::stdout(),
                    EnterAlternateScreen,
                    EnableBracketedPaste,
                    EnableMouseCapture,
                    Clear(ClearType::All)
                );
                let _ = enable_raw_mode();
            }
            _ => {}
        }
        return;
    }

    // Normal input handling
    match event {
        InputEvent::Tab => {
            state.focus = state.focus.next();
        }
        InputEvent::WorkbenchNextTab => {
            if state.focus != crate::tui::app::WorkspaceFocus::Workbench {
                state.focus = crate::tui::app::WorkspaceFocus::Workbench;
            }
            state.workbench_tab = state.workbench_tab.next();
        }
        InputEvent::InputChanged(c) => {
            match state.focus {
                crate::tui::app::WorkspaceFocus::Input => {
                    state.input.input(c);
                }
                crate::tui::app::WorkspaceFocus::Workbench => match state.workbench_tab {
                    crate::tui::app::WorkbenchTab::Approvals => {
                        match c {
                            'a' => {
                                if let Some(tc) = state
                                    .pending_approvals
                                    .get(state.approval_selected_idx)
                                    .cloned()
                                {
                                    state.pending_approvals.retain(|t| t.id != tc.id);
                                    state.approval_explanations.remove(&tc.id);
                                    state.approved_tools.push(tc.clone());
                                    state.approval_normalize_selection();
                                    let tool_name = tc.function.name.clone();
                                    let _ = output_tx.try_send(OutputEvent::AcceptTool(tc));
                                    state.push_activity(
                                        crate::tui::app::ActivityKind::Approval,
                                        format!("Approved: {}", tool_name),
                                    );
                                    state.toasts.push(crate::tui::services::Toast::success(
                                        format!("Approved: {}", tool_name),
                                    ));
                                }
                            }
                            'r' => {
                                if let Some(tc) = state
                                    .pending_approvals
                                    .get(state.approval_selected_idx)
                                    .cloned()
                                {
                                    state.pending_approvals.retain(|t| t.id != tc.id);
                                    state.approval_explanations.remove(&tc.id);
                                    state.rejected_tools.push(tc.clone());
                                    state.approval_normalize_selection();
                                    let tool_name = tc.function.name.clone();
                                    let _ = output_tx.try_send(OutputEvent::RejectTool(tc, false));
                                    state.push_activity(
                                        crate::tui::app::ActivityKind::Approval,
                                        format!("Rejected: {}", tool_name),
                                    );
                                    state.toasts.push(crate::tui::services::Toast::error(
                                        format!("Rejected: {}", tool_name),
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                    crate::tui::app::WorkbenchTab::Review => {}
                    crate::tui::app::WorkbenchTab::Sessions => {}
                },
                _ => {}
            }
        }
        InputEvent::InputChangedNewline => {
            if state.focus == crate::tui::app::WorkspaceFocus::Input {
                state.input.newline();
            }
        }
        InputEvent::InputBackspace => {
            if state.focus == crate::tui::app::WorkspaceFocus::Input {
                state.input.backspace();
            }
        }
        InputEvent::InputDelete => {
            if state.focus == crate::tui::app::WorkspaceFocus::Input {
                state.input.delete();
            }
        }
        InputEvent::InputClear => {
            if state.focus == crate::tui::app::WorkspaceFocus::Input {
                state.input.clear();
            }
        }
        InputEvent::InputSubmitted => {
            if state.focus == crate::tui::app::WorkspaceFocus::Workbench
                && state.workbench_tab == crate::tui::app::WorkbenchTab::Approvals
            {
                if let Some(tc) = state
                    .pending_approvals
                    .get(state.approval_selected_idx)
                    .cloned()
                {
                    state.pending_approvals.retain(|t| t.id != tc.id);
                    state.approval_explanations.remove(&tc.id);
                    state.approved_tools.push(tc.clone());
                    state.approval_normalize_selection();
                    let tool_name = tc.function.name.clone();
                    let _ = output_tx.try_send(OutputEvent::AcceptTool(tc));
                    state.push_activity(
                        crate::tui::app::ActivityKind::Approval,
                        format!("Approved: {}", tool_name),
                    );
                    state.toasts.push(crate::tui::services::Toast::success(format!(
                        "Approved: {}",
                        tool_name
                    )));
                }
                return;
            }

            if state.focus == crate::tui::app::WorkspaceFocus::Workbench
                && state.workbench_tab == crate::tui::app::WorkbenchTab::Sessions
            {
                if let Some(sel) = state.sessions.get(state.sessions_selected_idx).cloned() {
                    let _ = output_tx.try_send(OutputEvent::SwitchToSession(sel.id));
                    state.push_activity(crate::tui::app::ActivityKind::Session, "Switch session");
                }
                return;
            }

            if state.focus != crate::tui::app::WorkspaceFocus::Input || state.input.is_empty() {
                return;
            }

            if !state.input.is_empty() {
                let msg = state.input.get_content();
                state.input.clear();

                if msg.starts_with('/') {
                    let trimmed = msg.trim();
                    let mut parts = trimmed.splitn(2, char::is_whitespace);
                    let cmd_word = parts.next().unwrap_or(trimmed);
                    let cmd_args = parts.next().map(|s| s.trim()).filter(|s| !s.is_empty());

                    if let Some(cmd) = state
                        .commands
                        .iter()
                        .find(|c| c.command == cmd_word)
                        .cloned()
                    {
                        match cmd.source {
                            crate::tui::app::CommandSource::BuiltIn => {
                                if cmd.command == "/clear" {
                                    state.messages.clear();
                                    state.messages.extend(
                                        crate::tui::services::helper_block::welcome_messages(
                                            None, state,
                                        ),
                                    );
                                } else if cmd.command == "/sessions" {
                                    state.workbench_tab = crate::tui::app::WorkbenchTab::Sessions;
                                    state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                                    let _ = output_tx.try_send(OutputEvent::ListSessions);
                                } else if cmd.command == "/new" {
                                    let _ = output_tx.try_send(OutputEvent::NewSession);
                                } else if cmd.command == "/review" {
                                    state.add_user_message(trimmed.to_string());
                                    state.review_open = true;
                                    state.workbench_tab = crate::tui::app::WorkbenchTab::Review;
                                    state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                                    state.push_activity(crate::tui::app::ActivityKind::Review, "Open review");
                                    state.review_generation =
                                        state.review_generation.saturating_add(1);
                                    state.review_sync_items();
                                    state.review_normalize_selection();
                                } else if cmd.command == "/model" {
                                    state.add_user_message(trimmed.to_string());
                                    state.show_model_switcher = true;
                                    state.model_switcher_filter.clear();
                                    state.model_switcher_selected_idx = 0;
                                } else if cmd.command == "/files" {
                                    state.add_user_message(trimmed.to_string());
                                    state.show_file_search = true;
                                    state.file_search_query.clear();
                                    state.file_search_selected_idx = 0;
                                    if state.all_files.is_empty() {
                                        state.all_files =
                                            crate::tui::services::build_file_index(&state.project_root);
                                    }
                                    state.file_search_results = crate::tui::services::fuzzy_search_files(
                                        "",
                                        &state.all_files,
                                        50,
                                    );
                                } else if cmd.command == "/changes" {
                                    state.add_user_message(trimmed.to_string());
                                    state.show_changeset = true;
                                    state.changeset_selected_idx = 0;
                                    state.changeset_diff_scroll = 0;
                                    state.changeset_selected_path =
                                        state.modified_files.first().cloned();
                                    state.changeset_diff = state.changeset_selected_path.clone().map(|p| {
                                        let session_id = uuid::Uuid::parse_str(&state.session_id).ok();
                                        if let Some(session_id) = session_id {
                                            match crate::tui::services::review::load_diff(
                                                &state.project_root,
                                                session_id,
                                                &p,
                                            ) {
                                                Ok(diff) => crate::tui::app::ReviewDiffState {
                                                    path: diff.path,
                                                    old_content: Some(diff.old_content),
                                                    new_content: Some(diff.new_content),
                                                    scroll: 0,
                                                    last_error: None,
                                                },
                                                Err(e) => crate::tui::app::ReviewDiffState {
                                                    path: p,
                                                    old_content: None,
                                                    new_content: None,
                                                    scroll: 0,
                                                    last_error: Some(e),
                                                },
                                            }
                                        } else {
                                            crate::tui::app::ReviewDiffState {
                                                path: p,
                                                old_content: None,
                                                new_content: None,
                                                scroll: 0,
                                                last_error: Some("Invalid session id".to_string()),
                                            }
                                        }
                                    });
                                } else {
                                    state.add_user_message(trimmed.to_string());
                                    let _ = output_tx.try_send(OutputEvent::UserMessage(
                                        trimmed.to_string(),
                                        None,
                                        vec![],
                                        None,
                                    ));
                                }
                            }
                            crate::tui::app::CommandSource::BuiltInWithPrompt {
                                prompt_content,
                            }
                            | crate::tui::app::CommandSource::Custom { prompt_content } => {
                                state.add_user_message(trimmed.to_string());
                                let prompt = match cmd_args {
                                    Some(args) => format!("{prompt_content}\n\n{args}"),
                                    None => prompt_content,
                                };
                                let _ = output_tx.try_send(OutputEvent::UserMessage(
                                    prompt,
                                    None,
                                    vec![],
                                    None,
                                ));
                            }
                        }
                    } else {
                        state.add_user_message(msg.clone());
                        let _ =
                            output_tx.try_send(OutputEvent::UserMessage(msg, None, vec![], None));
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
            if state.focus == crate::tui::app::WorkspaceFocus::Input {
                state.input.move_cursor_left();
            }
        }
        InputEvent::CursorRight => {
            if state.focus == crate::tui::app::WorkspaceFocus::Input {
                state.input.move_cursor_right();
            }
        }
        InputEvent::Up => {
            match state.focus {
                crate::tui::app::WorkspaceFocus::Input => state.input.move_cursor_up(),
                crate::tui::app::WorkspaceFocus::Conversation => {
                    state.scroll = state.scroll.saturating_sub(1);
                }
                crate::tui::app::WorkspaceFocus::Activity => {
                    state.activity_scroll = state.activity_scroll.saturating_add(1);
                }
                crate::tui::app::WorkspaceFocus::Workbench => match state.workbench_tab {
                    crate::tui::app::WorkbenchTab::Approvals => {
                        state.approval_selected_idx = state.approval_selected_idx.saturating_sub(1);
                        state.approval_detail_scroll = 0;
                    }
                    crate::tui::app::WorkbenchTab::Sessions => {
                        state.sessions_selected_idx = state.sessions_selected_idx.saturating_sub(1);
                    }
                    crate::tui::app::WorkbenchTab::Review => {
                        let prev = state.review_selected_path.clone();
                        state.review_select_by_delta(-1);
                        if state.review_diff.is_some() && prev != state.review_selected_path {
                            if let (Some(path), Ok(session_id)) = (
                                state.review_selected_path.clone(),
                                uuid::Uuid::parse_str(&state.session_id),
                            ) {
                                match crate::tui::services::review::load_diff(
                                    &state.project_root,
                                    session_id,
                                    &path,
                                ) {
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
                },
            }
        }
        InputEvent::Down => {
            match state.focus {
                crate::tui::app::WorkspaceFocus::Input => state.input.move_cursor_down(),
                crate::tui::app::WorkspaceFocus::Conversation => {
                    state.scroll = state.scroll.saturating_add(1);
                }
                crate::tui::app::WorkspaceFocus::Activity => {
                    state.activity_scroll = state.activity_scroll.saturating_sub(1);
                }
                crate::tui::app::WorkspaceFocus::Workbench => match state.workbench_tab {
                    crate::tui::app::WorkbenchTab::Approvals => {
                        if state.approval_selected_idx + 1 < state.pending_approvals.len() {
                            state.approval_selected_idx += 1;
                            state.approval_detail_scroll = 0;
                        }
                    }
                    crate::tui::app::WorkbenchTab::Sessions => {
                        if state.sessions_selected_idx + 1 < state.sessions.len() {
                            state.sessions_selected_idx += 1;
                        }
                    }
                    crate::tui::app::WorkbenchTab::Review => {
                        let prev = state.review_selected_path.clone();
                        state.review_select_by_delta(1);
                        if state.review_diff.is_some() && prev != state.review_selected_path {
                            if let (Some(path), Ok(session_id)) = (
                                state.review_selected_path.clone(),
                                uuid::Uuid::parse_str(&state.session_id),
                            ) {
                                match crate::tui::services::review::load_diff(
                                    &state.project_root,
                                    session_id,
                                    &path,
                                ) {
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
                },
            }
        }
        InputEvent::InputCursorStart => {
            if state.focus == crate::tui::app::WorkspaceFocus::Input {
                state.input.move_cursor_start();
            }
        }
        InputEvent::InputCursorEnd => {
            if state.focus == crate::tui::app::WorkspaceFocus::Input {
                state.input.move_cursor_end();
            }
        }
        InputEvent::AttemptQuit => {
            state.cancel_requested = true;
        }
        InputEvent::HandleEsc => {
            if state.show_model_switcher {
                state.show_model_switcher = false;
                state.model_switcher_filter.clear();
                state.model_switcher_selected_idx = 0;
            } else if state.show_file_search {
                state.show_file_search = false;
                state.file_search_query.clear();
                state.file_search_selected_idx = 0;
                state.file_search_results.clear();
            } else if state.show_changeset {
                state.show_changeset = false;
                state.changeset_selected_idx = 0;
                state.changeset_diff_scroll = 0;
                state.changeset_selected_path = None;
                state.changeset_diff = None;
            } else if state.is_streaming {
                let _ = output_tx.try_send(OutputEvent::CancelStream);
                state.is_streaming = false;
            } else if state.focus == crate::tui::app::WorkspaceFocus::Workbench
                && state.workbench_tab == crate::tui::app::WorkbenchTab::Review
            {
                state.review_open = false;
                state.review_diff = None;
            }
        }
        InputEvent::AutoApproveCurrentTool => {
            if let Some(tc) = state
                .pending_approvals
                .get(state.approval_selected_idx)
                .cloned()
            {
                state.pending_approvals.retain(|t| t.id != tc.id);
                state.approval_explanations.remove(&tc.id);
                state.approved_tools.push(tc.clone());
                state.approval_normalize_selection();
                let tool_name = tc.function.name.clone();
                let _ = output_tx.try_send(OutputEvent::AcceptTool(tc));
                state.push_activity(
                    crate::tui::app::ActivityKind::Approval,
                    format!("Approved: {}", tool_name),
                );
                state.toasts.push(crate::tui::services::Toast::success(format!(
                    "Approved: {}",
                    tool_name
                )));
            }
        }
        InputEvent::RejectCurrentTool => {
            if let Some(tc) = state
                .pending_approvals
                .get(state.approval_selected_idx)
                .cloned()
            {
                state.pending_approvals.retain(|t| t.id != tc.id);
                state.approval_explanations.remove(&tc.id);
                state.rejected_tools.push(tc.clone());
                state.approval_normalize_selection();
                let tool_name = tc.function.name.clone();
                let _ = output_tx.try_send(OutputEvent::RejectTool(tc, false));
                state.push_activity(
                    crate::tui::app::ActivityKind::Approval,
                    format!("Rejected: {}", tool_name),
                );
                state.toasts.push(crate::tui::services::Toast::error(format!(
                    "Rejected: {}",
                    tool_name
                )));
            }
        }
        InputEvent::ScrollUp => {
            match state.focus {
                crate::tui::app::WorkspaceFocus::Conversation => {
                    state.scroll = state.scroll.saturating_sub(1);
                }
                crate::tui::app::WorkspaceFocus::Activity => {
                    state.activity_scroll = state.activity_scroll.saturating_add(1);
                }
                _ => {}
            }
        }
        InputEvent::ScrollDown => {
            match state.focus {
                crate::tui::app::WorkspaceFocus::Conversation => {
                    state.scroll = state.scroll.saturating_add(1);
                }
                crate::tui::app::WorkspaceFocus::Activity => {
                    state.activity_scroll = state.activity_scroll.saturating_sub(1);
                }
                _ => {}
            }
        }
        InputEvent::ShowCommandPalette => {
            state.show_command_palette = true;
            state.command_palette_input.clear();
            state.command_palette_selected = 0;
        }
        InputEvent::ShowModelSwitcher => {
            state.show_model_switcher = true;
            state.model_switcher_filter.clear();
            state.model_switcher_selected_idx = 0;
        }
        InputEvent::ShowFileSearch => {
            state.show_file_search = true;
            state.file_search_query.clear();
            state.file_search_selected_idx = 0;
            if state.all_files.is_empty() {
                state.all_files = crate::tui::services::build_file_index(&state.project_root);
            }
            state.file_search_results =
                crate::tui::services::fuzzy_search_files("", &state.all_files, 50);
        }
        InputEvent::ShowChangeset => {
            state.show_changeset = true;
            state.changeset_selected_idx = 0;
            state.changeset_diff_scroll = 0;
            state.changeset_selected_path = state.modified_files.first().cloned();
            state.changeset_diff = state.changeset_selected_path.clone().map(|p| {
                let session_id = uuid::Uuid::parse_str(&state.session_id).ok();
                if let Some(session_id) = session_id {
                    match crate::tui::services::review::load_diff(&state.project_root, session_id, &p)
                    {
                        Ok(diff) => crate::tui::app::ReviewDiffState {
                            path: diff.path,
                            old_content: Some(diff.old_content),
                            new_content: Some(diff.new_content),
                            scroll: 0,
                            last_error: None,
                        },
                        Err(e) => crate::tui::app::ReviewDiffState {
                            path: p,
                            old_content: None,
                            new_content: None,
                            scroll: 0,
                            last_error: Some(e),
                        },
                    }
                } else {
                    crate::tui::app::ReviewDiffState {
                        path: p,
                        old_content: None,
                        new_content: None,
                        scroll: 0,
                        last_error: Some("Invalid session id".to_string()),
                    }
                }
            });
        }
        InputEvent::ShowShortcuts => {
            state.show_shortcuts = true;
        }
        InputEvent::ToggleAutoApprove => {
            state.auto_approve = !state.auto_approve;
            if state.auto_approve {
                state.add_assistant_message(
                    "Permission Mode: AUTO-APPROVE (Low-risk tools will run without confirmation)"
                        .to_string(),
                );
            } else {
                state.add_assistant_message(
                    "Permission Mode: PROMPT (You will be prompted for tool execution)".to_string(),
                );
            }
        }
        InputEvent::RequestSessionList => {
            let _ = output_tx.try_send(OutputEvent::ListSessions);
        }
        InputEvent::NewSession => {
            let _ = output_tx.try_send(OutputEvent::NewSession);
        }
        InputEvent::ReviewOpen => {
            state.review_open = true;
            state.workbench_tab = crate::tui::app::WorkbenchTab::Review;
            state.focus = crate::tui::app::WorkspaceFocus::Workbench;
            state.push_activity(crate::tui::app::ActivityKind::Review, "Open review");
            state.review_generation = state.review_generation.saturating_add(1);
            state.review_sync_items();
            state.review_normalize_selection();
        }
        _ => {}
    }
}

/// Handle backend events
fn is_low_risk_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "file_read"
            | "file_write"
            | "file_edit"
            | "glob"
            | "grep"
            | "search"
            | "vil_knowledge"
            | "vil_diagnostics"
            | "vil_status"
            | "vil_lsp_query"
    )
}

fn handle_backend_event(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    match event {
        InputEvent::AssistantMessage(msg) => {
            state.add_assistant_message(msg);
            state.loading = false;
            state.push_activity(crate::tui::app::ActivityKind::Status, "Assistant message");
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
            let op_label = format!("{op:?}");
            state.loading_manager.start_operation(op);
            state.loading = true;
            state.push_activity(
                crate::tui::app::ActivityKind::Status,
                format!("Loading: {op_label}"),
            );
        }
        InputEvent::EndLoadingOperation(op) => {
            let op_label = format!("{op:?}");
            state.loading_manager.end_operation(op);
            state.loading = state.loading_manager.is_loading();
            state.is_streaming = false;
            state.push_activity(
                crate::tui::app::ActivityKind::Status,
                format!("Done: {op_label}"),
            );
        }
        InputEvent::Error(msg) => {
            state.add_assistant_message(format!("Error: {}", msg));
            state.toasts.push(crate::tui::services::Toast::error(msg.clone()));
            if state.toasts.len() > 3 {
                state.toasts.drain(0..state.toasts.len().saturating_sub(3));
            }
            state.loading = false;
            state.is_streaming = false;
            state.push_activity(crate::tui::app::ActivityKind::Error, msg);
        }
        InputEvent::SetCurrentModel(model) => {
            state.current_model = Some(model);
        }
        InputEvent::AvailableModelsLoaded(models) => {
            state.available_models = models;
        }
        InputEvent::ShowToast(toast) => {
            state.toasts.push(toast);
            if state.toasts.len() > 3 {
                state.toasts.drain(0..state.toasts.len().saturating_sub(3));
            }
        }
        InputEvent::SetSessions(sessions) => {
            state.sessions = sessions;
            state.sessions_selected_idx = 0;
            state.push_activity(crate::tui::app::ActivityKind::Session, "Sessions updated");
        }
        InputEvent::SessionRestored {
            id,
            title,
            messages,
        } => {
            state.session_id = id;
            state.session_title = Some(title);
            state.messages = messages;
            state.loading = false;

            // Clear transient UI state to prevent leakage between sessions
            state.pending_approvals.clear();
            state.pending_tool_calls.clear();
            state.approved_tools.clear();
            state.rejected_tools.clear();
            state.approval_explanations.clear();
            state.approval_selected_idx = 0;
            state.approval_detail_scroll = 0;
            state.is_streaming = false;
            state.streaming_message_id = None;
            state.scroll = 0;
            state.input.clear();
            state.review_open = false;
            state.review_filter.clear();
            state.review_diff = None;
            state.review_selected_idx = 0;
            state.review_selected_path = None;
            state.activity.clear();
            state.activity_scroll = 0;
            state.toasts.clear();
            state.show_model_switcher = false;
            state.model_switcher_filter.clear();
            state.model_switcher_selected_idx = 0;
            state.show_file_search = false;
            state.file_search_query.clear();
            state.file_search_selected_idx = 0;
            state.file_search_results.clear();
            state.show_changeset = false;
            state.changeset_selected_idx = 0;
            state.changeset_diff_scroll = 0;
            state.changeset_selected_path = None;
            state.changeset_store.clear();
            state.changeset_diff = None;
            state.workbench_tab = crate::tui::app::WorkbenchTab::Approvals;
            state.focus = crate::tui::app::WorkspaceFocus::Input;
            state.push_activity(crate::tui::app::ActivityKind::Session, "Session restored");
        }
        InputEvent::ShowConfirmationDialog(tc) => {
            state.pending_approvals.push(tc.clone());
            state.approval_selected_idx = state.pending_approvals.len().saturating_sub(1);
            state.approval_explanations.insert(tc.id.clone(), None);
            state.approval_normalize_selection();
            state.workbench_tab = crate::tui::app::WorkbenchTab::Approvals;
            state.focus = crate::tui::app::WorkspaceFocus::Workbench;
            state.push_activity(
                crate::tui::app::ActivityKind::Approval,
                format!("Approval required: {}", tc.function.name),
            );
        }
        InputEvent::ShowConfirmationDialogWithExplanation(tc, explanation) => {
            if state.auto_approve && is_low_risk_tool(&tc.function.name) {
                state.approved_tools.push(tc.clone());
                let _ = output_tx.try_send(OutputEvent::AcceptTool(tc));
                state.push_activity(
                    crate::tui::app::ActivityKind::Approval,
                    "Auto-approved low risk tool",
                );
            } else {
                state.pending_approvals.push(tc.clone());
                state.approval_selected_idx = state.pending_approvals.len().saturating_sub(1);
                state.approval_explanations.insert(tc.id.clone(), explanation);
                state.approval_normalize_selection();
                state.workbench_tab = crate::tui::app::WorkbenchTab::Approvals;
                state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                state.push_activity(
                    crate::tui::app::ActivityKind::Approval,
                    format!("Approval required: {}", tc.function.name),
                );
            }
        }
        InputEvent::RunToolCall(tc) => {
            state.pending_tool_calls.push(tc.clone());
            state.push_activity(
                crate::tui::app::ActivityKind::Tool,
                format!("Tool started: {}", tc.function.name),
            );
        }
        InputEvent::ToolResult(result) => {
            state.pending_tool_calls.retain(|c| c.id != result.call.id);
            state.approved_tools.retain(|c| c.id != result.call.id);
            state.add_assistant_message(result.result);
            state.push_activity(
                crate::tui::app::ActivityKind::Tool,
                format!("Tool result: {}", result.call.function.name),
            );
        }
        InputEvent::TaskCompleted(result) => {
            let mut content = result.summary.clone();
            if !result.modified_files.is_empty() {
                content.push_str("\n\n**Modified Files**:\n");
                for file in &result.modified_files {
                    content.push_str(&format!("- `{}`\n", file));
                    state.changeset_store.file_modified(
                        file.clone(),
                        "agent".to_string(),
                        true,
                    );
                    if !state.modified_files.contains(file) {
                        state.modified_files.push(file.clone());
                    }
                }
            }
            if !result.created_files.is_empty() {
                content.push_str("\n**Created Files**:\n");
                for file in &result.created_files {
                    content.push_str(&format!("- `{}`\n", file));
                    state.changeset_store.file_created(
                        file.clone(),
                        "agent".to_string(),
                    );
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
            state.push_activity(crate::tui::app::ActivityKind::Status, "Task completed");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::types::{FunctionCall, ToolCall};

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

        state
            .input
            .set_content("/explain crates/vac_cli/src/tui/event_loop.rs");
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
        assert_eq!(
            state.workbench_tab,
            crate::tui::app::WorkbenchTab::Review
        );
    }

    #[test]
    fn review_open_close_transitions_clear_diff() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.review_open = true;
        state.focus = crate::tui::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::tui::app::WorkbenchTab::Review;
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
        state.focus = crate::tui::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::tui::app::WorkbenchTab::Review;
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
    async fn approval_queue_accepts_selected_and_emits_output_event() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        let tc1 = ToolCall {
            id: "tc-1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "file_write".to_string(),
                arguments: serde_json::json!({"file_path":"a.txt","content":"x"}).to_string(),
            },
            metadata: None,
        };
        let tc2 = ToolCall {
            id: "tc-2".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "file_edit".to_string(),
                arguments: serde_json::json!({"file_path":"b.txt","old_string":"a","new_string":"b"}).to_string(),
            },
            metadata: None,
        };

        handle_backend_event(
            &mut state,
            &tx,
            InputEvent::ShowConfirmationDialogWithExplanation(tc1.clone(), Some("x".to_string())),
        );
        handle_backend_event(
            &mut state,
            &tx,
            InputEvent::ShowConfirmationDialogWithExplanation(tc2.clone(), Some("y".to_string())),
        );

        assert_eq!(state.pending_approvals.len(), 2);
        assert_eq!(state.approval_selected_idx, 1);
        assert_eq!(state.focus, crate::tui::app::WorkspaceFocus::Workbench);
        assert_eq!(state.workbench_tab, crate::tui::app::WorkbenchTab::Approvals);

        handle_input_event(&mut state, &tx, InputEvent::InputChanged('a'));
        let ev = rx.recv().await.unwrap();
        match ev {
            OutputEvent::AcceptTool(tc) => assert_eq!(tc.id, "tc-2"),
            _ => panic!("unexpected event"),
        }

        assert_eq!(state.pending_approvals.len(), 1);
        assert_eq!(state.pending_approvals[0].id, "tc-1");
        assert!(state.approved_tools.iter().any(|t| t.id == "tc-2"));
    }

    #[tokio::test]
    async fn approval_queue_rejects_selected_and_emits_output_event() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        let tc = ToolCall {
            id: "tc-1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "file_write".to_string(),
                arguments: serde_json::json!({"file_path":"a.txt","content":"x"}).to_string(),
            },
            metadata: None,
        };

        handle_backend_event(
            &mut state,
            &tx,
            InputEvent::ShowConfirmationDialogWithExplanation(tc.clone(), None),
        );

        handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));
        let ev = rx.recv().await.unwrap();
        match ev {
            OutputEvent::RejectTool(tc, _) => assert_eq!(tc.id, "tc-1"),
            _ => panic!("unexpected event"),
        }

        assert!(state.pending_approvals.is_empty());
        assert!(state.rejected_tools.iter().any(|t| t.id == "tc-1"));
    }

    #[tokio::test]
    async fn revert_selected_updates_status_and_working_tree() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let session_id = uuid::Uuid::new_v4();
        std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

        let file_rel = "a.txt";
        std::fs::write(root.join(file_rel), "new").unwrap();
        std::fs::write(
            crate::tui::services::review::snapshot_path(&root, session_id, file_rel),
            "old",
        )
        .unwrap();

        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(root.clone(), session_id);
        state.modified_files = vec![file_rel.to_string()];
        state.review_open = true;
        state.focus = crate::tui::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::tui::app::WorkbenchTab::Review;
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

        for (p, old, new) in [("a.txt", "old-a", "new-a"), ("b.txt", "old-b", "new-b")] {
            std::fs::write(root.join(p), new).unwrap();
            std::fs::write(
                crate::tui::services::review::snapshot_path(&root, session_id, p),
                old,
            )
            .unwrap();
        }

        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(root.clone(), session_id);
        state.modified_files = vec!["a.txt".to_string(), "b.txt".to_string()];
        state.review_open = true;
        state.focus = crate::tui::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::tui::app::WorkbenchTab::Review;
        state.review_filter = "a".to_string();
        state.review_sync_items();
        state.review_normalize_selection();

        handle_input_event(&mut state, &tx, InputEvent::ReviewRevertFiltered);

        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "old-a"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("b.txt")).unwrap(),
            "new-b"
        );
        assert!(!state.modified_files.contains(&"a.txt".to_string()));
        assert!(state.modified_files.contains(&"b.txt".to_string()));
    }

    #[tokio::test]
    async fn revert_all_clears_modified_files_and_marks_status() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let session_id = uuid::Uuid::new_v4();
        std::fs::create_dir_all(root.join(".vac/backups").join(session_id.to_string())).unwrap();

        for (p, old, new) in [("a.txt", "old-a", "new-a"), ("b.txt", "old-b", "new-b")] {
            std::fs::write(root.join(p), new).unwrap();
            std::fs::write(
                crate::tui::services::review::snapshot_path(&root, session_id, p),
                old,
            )
            .unwrap();
        }

        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(root.clone(), session_id);
        state.modified_files = vec!["a.txt".to_string(), "b.txt".to_string()];
        state.review_open = true;
        state.focus = crate::tui::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::tui::app::WorkbenchTab::Review;
        state.review_sync_items();
        state.review_normalize_selection();

        handle_input_event(&mut state, &tx, InputEvent::ReviewRevertAll);

        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "old-a"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("b.txt")).unwrap(),
            "old-b"
        );
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
