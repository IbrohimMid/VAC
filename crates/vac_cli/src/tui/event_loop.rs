//! Event Loop Module

use crate::tui::app::{AppState, AppStateOptions, InputEvent, LoadingOperation, OutputEvent};
use crate::tui::event::map_crossterm_event_to_input_event;
use crate::tui::services::helper_block::welcome_messages;
use crate::tui::terminal::TerminalGuard;
use crate::tui::view::view;
use crate::tui::Model;
use crossterm::{
    event::{EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
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
    _editor_command: Option<String>,
    _auth_display_info: (Option<String>, Option<String>, Option<String>),
    _init_prompt_content: Option<String>,
    _send_init_prompt_on_start: bool,
    _recent_models: Vec<String>,
    _banner_message: Option<()>,
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
        session_id: None,
        checkpoint_path: checkpoint_path.clone(),
    });

    // Add welcome messages
    let welcome = welcome_messages(latest_version.as_deref(), &state);
    state.messages.extend(welcome);

    // Request session restore if checkpoint exists
    if let Some(path) = checkpoint_path {
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
            handle_backend_event(&mut state, event);
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
                if let Some(cmd) = filtered.get(state.command_palette_selected) {
                    let _ = output_tx.try_send(OutputEvent::ExecuteCommand(cmd.command.clone()));
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
                state.is_dialog_open = false;
                state.dialog_command = None;
            }
            InputEvent::DialogUp => {
                if state.dialog_selected > 0 {
                    state.dialog_selected -= 1;
                }
            }
            InputEvent::DialogDown => {
                state.dialog_selected = (state.dialog_selected + 1) % 3;
            }
            InputEvent::DialogSelect => match state.dialog_selected {
                0 => {
                    if let Some(tc) = state.dialog_command.take() {
                        let _ = output_tx.try_send(OutputEvent::AcceptTool(tc));
                    }
                    state.is_dialog_open = false;
                }
                1 => {
                    if let Some(tc) = state.dialog_command.take() {
                        let _ = output_tx.try_send(OutputEvent::RejectTool(tc, false));
                    }
                    state.is_dialog_open = false;
                }
                _ => {
                    state.is_dialog_open = false;
                }
            },
            InputEvent::ApproveTool => {
                if let Some(tc) = state.dialog_command.take() {
                    let _ = output_tx.try_send(OutputEvent::AcceptTool(tc));
                }
                state.is_dialog_open = false;
            }
            InputEvent::RejectTool => {
                if let Some(tc) = state.dialog_command.take() {
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

    // Normal input handling
    match event {
        InputEvent::InputChanged(c) => {
            state.input.push(c);
        }
        InputEvent::InputBackspace => {
            state.input.pop();
        }
        InputEvent::InputDelete => {
            state.input.clear();
        }
        InputEvent::InputSubmitted => {
            if !state.input.is_empty() {
                let msg = state.input.clone();
                state.add_user_message(msg.clone());
                state.input.clear();
                let _ = output_tx.try_send(OutputEvent::UserMessage(msg, None, vec![], None));
            }
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
fn handle_backend_event(state: &mut AppState, event: InputEvent) {
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
        }
        InputEvent::ShowConfirmationDialog(tc) => {
            state.dialog_command = Some(tc);
            state.is_dialog_open = true;
            state.dialog_selected = 0;
        }
        InputEvent::RunToolCall(tc) => {
            state.pending_tool_calls.push(tc);
        }
        InputEvent::ToolResult(result) => {
            state.add_assistant_message(result.result);
        }
        _ => {}
    }
}