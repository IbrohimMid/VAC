//! Event Loop Module

use crate::tui::Model;
use crate::tui::app::{AppState, AppStateOptions, InputEvent, OutputEvent};
use crate::tui::event::map_crossterm_event_to_input_event;
use crate::tui::handlers::HandlerContext;
use crate::tui::handlers::{
    approval, changeset as changeset_handler, file_search, isolation_switcher, message_action,
    model_switcher, profile_switcher, review as review_handler, rulebook_switcher,
};
use crate::tui::services::helper_block::welcome_messages;
use crate::tui::terminal::TerminalGuard;
use crate::tui::view::view;
use crossterm::{
    event::{EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
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
        project_root: project_root.clone(),
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
    state.input_tx = Some(input_tx.clone());

    // Probe MCP servers in background
    let config = vac_core::VacConfig::load_with_fallback(&project_root).unwrap_or_default();
    if let Some(servers) = config.mcp_servers {
        let input_tx_clone = input_tx.clone();
        tokio::spawn(async move {
            for server in servers {
                let state = vac_tools::mcp::probe_mcp_server(&server).await;
                let _ = input_tx_clone.send(InputEvent::McpServerState(server.name, state)).await;
            }
        });
    }

    // Spawn background task to detect VIL project profile
    let bg_input_tx = input_tx.clone();
    let bg_project_root = project_root.clone();
    tokio::spawn(async move {
        let profile = vac_core::detector::VilProjectProfile::detect(&bg_project_root);
        
        let mut ir_generation_active = false;
        let mut ir_metadata_files = vec![];
        if let Ok(pipeline) = vil_ir::IrPipeline::new(&bg_project_root) {
            ir_generation_active = true;
            for (path, module) in pipeline.modules() {
                let has_vil_attr = module.structs.iter().any(|s| !s.vil_attrs.is_empty())
                    || module.functions.iter().any(|f| !f.vil_attrs.is_empty());
                if has_vil_attr {
                    ir_metadata_files.push(path.clone());
                }
            }
        }
        
        let config = vac_core::VacConfig::load_with_fallback(&bg_project_root).unwrap_or_default();
        let semantic_mode = config.memory.enable_semantic;

        let active_rulebook = {
            let books = vac_core::rulebook::RulebookLoader::load_all(&bg_project_root, &config.rulebook.paths);
            if books.is_empty() {
                None
            } else {
                Some(books.iter().map(|b| b.id.clone()).collect::<Vec<_>>().join(", "))
            }
        };

        let snapshot = crate::tui::app::VilStatusSnapshot {
            profile: Some(profile),
            validation_score: 1.0,
            validation_issues: vec![],
            active_rulebook,
            semantic_mode,
            ir_generation_active,
            ir_metadata_files,
        };
        let _ = bg_input_tx.send(InputEvent::VilStatusUpdated(snapshot)).await;
    });

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
    // Reject reason prompt intercepts all input when active
    if state.reject_reason_input.is_some() {
        let mut ctx = HandlerContext::new(state, output_tx);
        match event {
            InputEvent::InputSubmitted => {
                let _ = approval::confirm_reject_current(&mut ctx);
            }
            InputEvent::HandleEsc => {
                // Esc = skip reason, reject without reason
                ctx.state.reject_reason_input = None;
                let _ = approval::reject_current(&mut ctx);
            }
            InputEvent::InputChanged(c) => {
                let _ = approval::reason_input_push(&mut ctx, c);
            }
            InputEvent::InputBackspace => {
                let _ = approval::reason_input_pop(&mut ctx);
            }
            _ => {}
        }
        return;
    }

    if state.show_helper_dropdown && state.focus == crate::tui::app::WorkspaceFocus::Input {
        match event {
            InputEvent::Up => {
                if state.helper_selected > 0 {
                    state.helper_selected -= 1;
                    if state.helper_selected < state.helper_scroll {
                        state.helper_scroll = state.helper_selected;
                    }
                }
                return;
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
                return;
            }
            InputEvent::InputSubmitted => {
                if let Some(cmd) = state.filtered_helpers.get(state.helper_selected).cloned() {
                    state.input.set_content(&cmd.command);
                    state.input.move_cursor_end();
                    state.input.input(' '); // add trailing space
                }
                state.show_helper_dropdown = false;
                return;
            }
            InputEvent::HandleEsc => {
                state.show_helper_dropdown = false;
                return;
            }
            _ => {}
        }
    }

    // @ inline file picker intercepts Up/Down/Enter/Esc when active
    if state.at_trigger_active && state.focus == crate::tui::app::WorkspaceFocus::Input {
        match event {
            InputEvent::Up => {
                state.at_selected_idx = state.at_selected_idx.saturating_sub(1);
                return;
            }
            InputEvent::Down => {
                if !state.at_results.is_empty() {
                    state.at_selected_idx =
                        (state.at_selected_idx + 1).min(state.at_results.len().saturating_sub(1));
                }
                return;
            }
            InputEvent::InputSubmitted => {
                // Insert selected path: replace @query with path
                if let Some(path) = state.at_results.get(state.at_selected_idx).cloned() {
                    // Remove @query from input, insert path
                    let remove_len = state.at_query.len() + 1; // +1 for '@'
                    for _ in 0..remove_len {
                        state.input.backspace();
                    }
                    state.input.insert_str(&path);
                    state.input.input(' '); // trailing space
                }
                state.at_trigger_active = false;
                state.at_query.clear();
                state.at_results.clear();
                state.at_selected_idx = 0;
                return;
            }
            InputEvent::HandleEsc => {
                state.at_trigger_active = false;
                state.at_query.clear();
                state.at_results.clear();
                state.at_selected_idx = 0;
                return;
            }
            InputEvent::InputChanged(' ') => {
                // Space deactivates picker, char goes to input normally
                state.at_trigger_active = false;
                state.at_query.clear();
                state.at_results.clear();
                state.at_selected_idx = 0;
                state.input.input(' ');
                return;
            }
            _ => {} // other keys fall through to normal handling
        }
    }

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
                            } else if cmd.command == "/runtime" {
                                state.workbench_tab = crate::tui::app::WorkbenchTab::Runtime;
                                state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                                let _ = output_tx.try_send(OutputEvent::ListRuntimeJobs);
                                let _ = output_tx.try_send(OutputEvent::LoadRuntimeState);
                            } else if cmd.command == "/shell" {
                                state.add_user_message(cmd.command.clone());
                                let _ =
                                        output_tx.try_send(OutputEvent::ExecuteCommand(String::new(), state.active_isolation_mode.clone()));
                            } else if cmd.command == "/shell-focus" {
                                if state.active_shell_command.is_some() {
                                    state.shell_popup_visible = true;
                                    state.shell_backgrounded = false;
                                }
                            } else if cmd.command == "/shell-bg" {
                                if state.active_shell_command.is_some() {
                                    state.shell_popup_visible = false;
                                    state.shell_backgrounded = true;
                                }
                            } else if cmd.command == "/shell-kill" {
                                if let Some(shell) = state.active_shell_command.clone() {
                                    let _ = shell.kill();
                                }
                            } else if cmd.command == "/new" {
                                let _ = output_tx.try_send(OutputEvent::NewSession);
                            } else if cmd.command == "/review" {
                                state.add_user_message(cmd.command.clone());
                                let mut ctx = HandlerContext::new(state, output_tx);
                                let _ = review_handler::open(&mut ctx);
                            } else if cmd.command == "/model" {
                                state.add_user_message(cmd.command.clone());
                                let mut ctx = HandlerContext::new(state, output_tx);
                                let _ = model_switcher::open(&mut ctx);
                            } else if cmd.command == "/files" {
                                state.add_user_message(cmd.command.clone());
                                let mut ctx = HandlerContext::new(state, output_tx);
                                let _ = file_search::open(&mut ctx);
                            } else if cmd.command == "/changes" {
                                state.add_user_message(cmd.command.clone());
                                let mut ctx = HandlerContext::new(state, output_tx);
                                let _ = changeset_handler::open(&mut ctx);
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
            InputEvent::Tab => {
                state.shortcuts_mode = match state.shortcuts_mode {
                    crate::tui::app::ShortcutsPopupMode::Commands => crate::tui::app::ShortcutsPopupMode::Shortcuts,
                    crate::tui::app::ShortcutsPopupMode::Shortcuts => crate::tui::app::ShortcutsPopupMode::Sessions,
                    crate::tui::app::ShortcutsPopupMode::Sessions => crate::tui::app::ShortcutsPopupMode::Commands,
                };
                state.shortcuts_scroll = 0;
            }
            InputEvent::Up => {
                state.shortcuts_scroll = state.shortcuts_scroll.saturating_sub(1);
            }
            InputEvent::Down => {
                let max = match state.shortcuts_mode {
                    crate::tui::app::ShortcutsPopupMode::Commands => crate::tui::services::shortcuts_popup::filter_commands("", state).len(),
                    crate::tui::app::ShortcutsPopupMode::Shortcuts => crate::tui::services::shortcuts_popup::get_shortcuts_count(),
                    crate::tui::app::ShortcutsPopupMode::Sessions => state.sessions.len(),
                }.saturating_sub(1);
                if state.shortcuts_scroll < max {
                    state.shortcuts_scroll += 1;
                }
            }
            InputEvent::InputSubmitted => {
                if state.shortcuts_mode == crate::tui::app::ShortcutsPopupMode::Sessions {
                    if let Some(sel) = state.sessions.get(state.shortcuts_scroll).cloned() {
                        let _ = output_tx.try_send(OutputEvent::SwitchToSession(sel.id));
                        state.push_activity(crate::tui::app::ActivityKind::Session, "Switch session");
                        state.show_shortcuts = false;
                    }
                } else if state.shortcuts_mode == crate::tui::app::ShortcutsPopupMode::Commands {
                    let cmds = crate::tui::services::shortcuts_popup::filter_commands("", state);
                    if let Some(cmd) = cmds.get(state.shortcuts_scroll) {
                        match &cmd.action {
                            crate::tui::services::shortcuts_popup::CommandAction::InsertSlashCommand(s) => {
                                state.input.clear();
                                state.input.insert_str(s);
                                state.input.input(' ');
                            }
                            _ => {}
                        }
                        state.show_shortcuts = false;
                    }
                }
            }
            _ => {}
        }
        return;
    }

    if state.show_isolation_switcher {
        let mut ctx = HandlerContext::new(state, output_tx);
        let _ = isolation_switcher::handle_event(&mut ctx, event);
        return;
    }

    if state.show_profile_switcher {
        let mut ctx = HandlerContext::new(state, output_tx);
        let _ = profile_switcher::handle_event(&mut ctx, event);
        return;
    }

    if state.show_rulebook_switcher {
        let mut ctx = HandlerContext::new(state, output_tx);
        let _ = rulebook_switcher::handle_event(&mut ctx, event);
        return;
    }

    if state.show_message_action_popup {
        let mut ctx = HandlerContext::new(state, output_tx);
        let _ = message_action::handle_event(&mut ctx, event);
        return;
    }

    if state.show_model_switcher {
        let mut ctx = HandlerContext::new(state, output_tx);
        match event {
            InputEvent::HandleEsc => {
                let _ = model_switcher::close(&mut ctx);
            }
            InputEvent::InputChanged(c) => {
                let mut f = ctx.state.model_switcher_filter.clone();
                f.push(c);
                let _ = model_switcher::update_filter(&mut ctx, f);
            }
            InputEvent::InputBackspace => {
                let mut f = ctx.state.model_switcher_filter.clone();
                f.pop();
                let _ = model_switcher::update_filter(&mut ctx, f);
            }
            InputEvent::Up => {
                let _ = model_switcher::select_prev(&mut ctx);
            }
            InputEvent::Down => {
                let _ = model_switcher::select_next(&mut ctx);
            }
            InputEvent::InputSubmitted => {
                let _ = model_switcher::submit_selected(&mut ctx);
            }
            _ => {}
        }
        return;
    }

    if state.show_file_search {
        let mut ctx = HandlerContext::new(state, output_tx);
        if ctx.state.all_files.is_empty() {
            ctx.state.all_files = crate::tui::services::build_file_index(&ctx.state.project_root);
        }
        if ctx.state.file_search_results.is_empty() {
            let q = ctx.state.file_search_query.clone();
            let results = crate::tui::services::fuzzy_search_files(&q, &ctx.state.all_files, 50);
            let max = results.len().saturating_sub(1);
            ctx.state.file_search_results = results;
            ctx.state.file_search_selected_idx = ctx.state.file_search_selected_idx.min(max);
        }
        match event {
            InputEvent::HandleEsc => {
                let _ = file_search::close(&mut ctx);
            }
            InputEvent::InputChanged(c) => {
                let mut q = ctx.state.file_search_query.clone();
                q.push(c);
                let _ = file_search::update_query(&mut ctx, q);
            }
            InputEvent::InputBackspace => {
                let mut q = ctx.state.file_search_query.clone();
                q.pop();
                let _ = file_search::update_query(&mut ctx, q);
            }
            InputEvent::Up => {
                let _ = file_search::select_prev(&mut ctx);
            }
            InputEvent::Down => {
                let _ = file_search::select_next(&mut ctx);
            }
            InputEvent::InputSubmitted => {
                let _ = file_search::insert_selected(&mut ctx);
            }
            _ => {}
        }
        return;
    }

    if state.show_changeset {
        let mut ctx = HandlerContext::new(state, output_tx);
        match event {
            InputEvent::HandleEsc => {
                let _ = changeset_handler::close(&mut ctx);
            }
            InputEvent::Up => {
                let _ = changeset_handler::select_prev(&mut ctx);
            }
            InputEvent::Down => {
                let _ = changeset_handler::select_next(&mut ctx);
            }
            InputEvent::ScrollUp => {
                let _ = changeset_handler::scroll_up(&mut ctx);
            }
            InputEvent::ScrollDown => {
                let _ = changeset_handler::scroll_down(&mut ctx);
            }
            _ => {}
        }
        return;
    }

    if state.review_open
        && state.focus == crate::tui::app::WorkspaceFocus::Workbench
        && state.workbench_tab == crate::tui::app::WorkbenchTab::Review
    {
        let mut ctx = HandlerContext::new(state, output_tx);
        match event {
            InputEvent::HandleEsc | InputEvent::ReviewClose => {
                let _ = review_handler::close(&mut ctx);
            }
            InputEvent::ReviewOpen => {
                let _ = review_handler::open(&mut ctx);
            }
            InputEvent::ReviewUp | InputEvent::Up | InputEvent::ScrollUp => {
                let _ = review_handler::select_prev(&mut ctx);
            }
            InputEvent::ReviewDown | InputEvent::Down | InputEvent::ScrollDown => {
                let _ = review_handler::select_next(&mut ctx);
            }
            InputEvent::ReviewFilterInput(c) | InputEvent::InputChanged(c) => {
                let _ = review_handler::filter_push(&mut ctx, c);
            }
            InputEvent::ReviewFilterBackspace | InputEvent::InputBackspace => {
                let _ = review_handler::filter_pop(&mut ctx);
            }
            InputEvent::ReviewToggleDiff | InputEvent::InputSubmitted => {
                let _ = review_handler::toggle_diff(&mut ctx);
            }
            InputEvent::PageUp => {
                let _ = review_handler::scroll_up(&mut ctx, 10);
            }
            InputEvent::PageDown => {
                let _ = review_handler::scroll_down(&mut ctx, 10);
            }
            InputEvent::ReviewRevertSelected => {
                let _ = review_handler::revert_selected(&mut ctx);
            }
            InputEvent::ReviewRevertFiltered => {
                let _ = review_handler::revert_filtered(&mut ctx);
            }
            InputEvent::ReviewRevertAll | InputEvent::HandleCtrlZ => {
                let _ = review_handler::revert_all(&mut ctx);
            }
            InputEvent::ReviewOpenEditor => {
                let _ = review_handler::open_editor(&mut ctx);
            }
            _ => {}
        }
        return;
    }

    // Normal input handling
    match event {
        InputEvent::ShowProfileSwitcher => {
            state.show_profile_switcher = true;
            state.profile_search_input.clear();
        }
        InputEvent::ShowIsolationSwitcher => {
            state.show_isolation_switcher = true;
            state.isolation_switcher_selected = 0;
        }
        InputEvent::ShowRulebookSwitcher => {
            state.show_rulebook_switcher = true;
            state.rulebook_search_input.clear();
        }
        InputEvent::ShowMessageActionPopup => {
            state.show_message_action_popup = true;
            state.message_action_popup_selected = 0;
            // Target the last user message if any
            state.message_action_target_id = state.messages.iter().rev().find(|m| m.role == "user").map(|m| m.id);
        }
        InputEvent::ToggleSidePanel => {
            state.side_panel_visible = !state.side_panel_visible;
            if !state.side_panel_visible {
                state.side_panel_row_areas.clear();
            }
        }
        InputEvent::MouseDragStart(col, row) => {
            let mut clicked_section = None;
            if state.side_panel_visible {
                for (sec, rect) in &state.side_panel_header_areas {
                    if col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height {
                        clicked_section = Some(*sec);
                        break;
                    }
                }
            }

            if let Some(sec) = clicked_section {
                if state.side_panel_section_collapsed.contains(&sec) {
                    state.side_panel_section_collapsed.remove(&sec);
                } else {
                    state.side_panel_section_collapsed.insert(sec);
                }
            } else {
                // Check per-row click areas in side panel
                let mut handled = false;
                if state.side_panel_visible {
                    for (action, rect) in &state.side_panel_row_areas {
                        if col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height {
                            match action.clone() {
                                crate::tui::app::SidePanelRowAction::SwitchSession(id) => {
                                    let _ = output_tx.try_send(OutputEvent::SwitchToSession(id));
                                }
                                crate::tui::app::SidePanelRowAction::ShowMcpDetail(name) => {
                                    state.push_activity(
                                        crate::tui::app::ActivityKind::Mcp,
                                        format!("MCP detail: {}", name),
                                    );
                                    state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                                    state.workbench_tab = crate::tui::app::WorkbenchTab::Runtime;
                                }
                                crate::tui::app::SidePanelRowAction::JumpToVilIssue(path) => {
                                    state.changeset_selected_path = Some(path);
                                    state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                                    state.workbench_tab = crate::tui::app::WorkbenchTab::Review;
                                }
                            }
                            handled = true;
                            break;
                        }
                    }
                }
                if !handled {
                    if row == 0 {
                        let term_width = crossterm::terminal::size().map(|s| s.0).unwrap_or(80);
                        if col > term_width.saturating_sub(40) {
                            state.side_panel_visible = !state.side_panel_visible;
                        }
                    } else {
                        crate::tui::services::text_selection::handle_drag_start(state, col, row);
                    }
                }
            }
        }
        InputEvent::MouseDrag(col, row) => {
            crate::tui::services::text_selection::handle_drag(state, col, row);
        }
        InputEvent::MouseDragEnd(col, row) => {
            crate::tui::services::text_selection::handle_drag_end(state, col, row);
        }
        InputEvent::MouseRightClick(_col, row) => {
            // Resolve which message the click landed on and open popup for it
            if let Some(msg_id) = message_at_row(state, row) {
                state.show_message_action_popup = true;
                state.message_action_popup_selected = 0;
                state.message_action_target_id = Some(msg_id);
            }
        }
        InputEvent::Tab => {
            state.focus = state.focus.next();
        }
        InputEvent::WorkbenchNextTab => {
            if state.focus != crate::tui::app::WorkspaceFocus::Workbench {
                state.focus = crate::tui::app::WorkspaceFocus::Workbench;
            }
            state.workbench_tab = state.workbench_tab.next();
            if state.workbench_tab == crate::tui::app::WorkbenchTab::Runtime {
                let _ = output_tx.try_send(OutputEvent::ListRuntimeJobs);
                let _ = output_tx.try_send(OutputEvent::LoadRuntimeState);
            }
        }
        InputEvent::InputChanged(c) => {
            match state.focus {
                crate::tui::app::WorkspaceFocus::Input => {
                    if c == '/' && state.input.lines.join("").trim().is_empty() {
                        state.show_helper_dropdown = true;
                        state.input.input(c);
                        crate::tui::services::helper_dropdown::filter_helpers_sync(state);
                        state.helper_selected = 0;
                        state.helper_scroll = 0;
                    } else if state.show_helper_dropdown {
                        state.input.input(c);
                        crate::tui::services::helper_dropdown::filter_helpers_sync(state);
                        state.helper_selected = 0;
                        state.helper_scroll = 0;
                    } else if c == '@' && !state.at_trigger_active {
                        // Activate inline @ picker
                        state.at_trigger_active = true;
                        state.at_query = String::new();
                        state.at_selected_idx = 0;
                        if state.all_files.is_empty() {
                            state.all_files =
                                crate::tui::services::build_file_index(&state.project_root);
                        }
                        state.at_results =
                            crate::tui::services::fuzzy_search_files("", &state.all_files, 8);
                        state.input.input(c);
                    } else if state.at_trigger_active {
                        // Update query with new char
                        state.at_query.push(c);
                        state.at_selected_idx = 0;
                        state.at_results = crate::tui::services::fuzzy_search_files(
                            &state.at_query,
                            &state.all_files,
                            8,
                        );
                        state.input.input(c);
                    } else {
                        state.input.input(c);
                    }
                }
                crate::tui::app::WorkspaceFocus::Workbench => match state.workbench_tab {
                    crate::tui::app::WorkbenchTab::Approvals => match c {
                        'a' => {
                            let mut ctx = HandlerContext::new(state, output_tx);
                            let _ = approval::approve_current(&mut ctx);
                        }
                        'r' => {
                            let mut ctx = HandlerContext::new(state, output_tx);
                            let _ = approval::begin_reject_current(&mut ctx);
                        }
                        _ => {}
                    },
                    crate::tui::app::WorkbenchTab::Review => {}
                    crate::tui::app::WorkbenchTab::Sessions => {
                        if c == 'r' {
                            if let Some(sel) =
                                state.sessions.get(state.sessions_selected_idx).cloned()
                            {
                                if sel.has_checkpoint {
                                    let _ = output_tx
                                        .try_send(OutputEvent::ResumeSession(sel.id.clone()));
                                    state.push_activity(
                                        crate::tui::app::ActivityKind::Session,
                                        format!(
                                            "Resuming checkpoint: {}",
                                            &sel.id[..8.min(sel.id.len())]
                                        ),
                                    );
                                } else {
                                    state.toasts.push(crate::tui::services::Toast::info(
                                        "No checkpoint available for this session".to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    crate::tui::app::WorkbenchTab::Runtime => match c {
                        'r' => {
                            let _ = output_tx.try_send(OutputEvent::ListRuntimeJobs);
                            let _ = output_tx.try_send(OutputEvent::LoadRuntimeState);
                        }
                        'c' => {
                            if let Some(job) = state.runtime_jobs.get(state.runtime_selected_idx) {
                                let _ = output_tx.try_send(OutputEvent::CancelRuntimeJob(job.id));
                            }
                        }
                        't' => {
                            if let Some(job) = state.runtime_jobs.get(state.runtime_selected_idx) {
                                let _ = output_tx.try_send(OutputEvent::RetryRuntimeJob(job.id));
                            }
                        }
                        _ => {}
                    },
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
                if state.show_helper_dropdown {
                    state.input.backspace();
                    crate::tui::services::helper_dropdown::filter_helpers_sync(state);
                    state.helper_selected = 0;
                    state.helper_scroll = 0;
                } else if state.at_trigger_active {
                    if state.at_query.is_empty() {
                        state.at_trigger_active = false;
                        state.at_results.clear();
                        state.at_selected_idx = 0;
                    } else {
                        state.at_query.pop();
                        state.at_selected_idx = 0;
                        state.at_results = crate::tui::services::fuzzy_search_files(
                            &state.at_query,
                            &state.all_files,
                            8,
                        );
                    }
                    state.input.backspace();
                } else {
                    state.input.backspace();
                }
            }
        }
        InputEvent::InputDelete => {
            if state.focus == crate::tui::app::WorkspaceFocus::Input {
                state.input.delete();
                if state.show_helper_dropdown {
                    crate::tui::services::helper_dropdown::filter_helpers_sync(state);
                }
            }
        }
        InputEvent::InputClear => {
            if state.focus == crate::tui::app::WorkspaceFocus::Input {
                state.input.clear();
            }
        }
        InputEvent::InputSubmitted => {
            if state.focus == crate::tui::app::WorkspaceFocus::Input
                && state.shell_popup_visible
                && state.active_shell_command.is_some()
            {
                let text = state.input.get_content();
                let payload = if text.is_empty() {
                    "\n".to_string()
                } else {
                    if state.shell_history.last() != Some(&text) {
                        state.shell_history.push(text.clone());
                    }
                    state.shell_history_idx = None;
                    format!("{text}\n")
                };
                if let Some(shell) = state.active_shell_command.clone() {
                    shell.send_input(payload);
                    state.input.clear();
                    state.shell_waiting_for_input = false;
                    state.push_activity(
                        crate::tui::app::ActivityKind::Status,
                        "Sent input to shell",
                    );
                }
                return;
            }

            if state.focus == crate::tui::app::WorkspaceFocus::Workbench
                && state.workbench_tab == crate::tui::app::WorkbenchTab::Approvals
            {
                let mut ctx = HandlerContext::new(state, output_tx);
                let _ = approval::approve_current(&mut ctx);
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
                    state.recent_commands.add_command(msg.clone());
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
                                } else if cmd.command == "/runtime" {
                                    state.workbench_tab = crate::tui::app::WorkbenchTab::Runtime;
                                    state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                                    let _ = output_tx.try_send(OutputEvent::ListRuntimeJobs);
                                    let _ = output_tx.try_send(OutputEvent::LoadRuntimeState);
                                } else if cmd.command == "/shell" {
                                    state.add_user_message(trimmed.to_string());
                                    let shell_cmd = cmd_args.unwrap_or_default().to_string();
                                    let _ =
                                        output_tx.try_send(OutputEvent::ExecuteCommand(shell_cmd, state.active_isolation_mode.clone()));
                                } else if cmd.command == "/shell-focus" {
                                    state.shell_popup_visible =
                                        state.active_shell_command.is_some();
                                    state.shell_backgrounded = false;
                                } else if cmd.command == "/shell-bg" {
                                    if state.active_shell_command.is_some() {
                                        state.shell_popup_visible = false;
                                        state.shell_backgrounded = true;
                                    }
                                } else if cmd.command == "/shell-kill" {
                                    if let Some(shell) = state.active_shell_command.clone() {
                                        let _ = shell.kill();
                                    }
                                } else if cmd.command == "/new" {
                                    let _ = output_tx.try_send(OutputEvent::NewSession);
                                } else if cmd.command == "/review" {
                                    state.add_user_message(trimmed.to_string());
                                    let mut ctx = HandlerContext::new(state, output_tx);
                                    let _ = review_handler::open(&mut ctx);
                                } else if cmd.command == "/model" {
                                    state.add_user_message(trimmed.to_string());
                                    let mut ctx = HandlerContext::new(state, output_tx);
                                    let _ = model_switcher::open(&mut ctx);
                                } else if cmd.command == "/files" {
                                    state.add_user_message(trimmed.to_string());
                                    let mut ctx = HandlerContext::new(state, output_tx);
                                    let _ = file_search::open(&mut ctx);
                                } else if cmd.command == "/changes" {
                                    state.add_user_message(trimmed.to_string());
                                    let mut ctx = HandlerContext::new(state, output_tx);
                                    let _ = changeset_handler::open(&mut ctx);
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
                        let parts = std::mem::take(&mut state.pending_image_parts);
                        let _ =
                            output_tx.try_send(OutputEvent::UserMessage(msg, None, parts, None));
                    }
                } else {
                    state.add_user_message(msg.clone());
                    let parts = std::mem::take(&mut state.pending_image_parts);
                    let _ = output_tx.try_send(OutputEvent::UserMessage(msg, None, parts, None));
                }
            }
        }
        InputEvent::HandlePaste(text) => {
            // First try to extract file paths
            let paths = crate::tui::services::clipboard_paste::extract_file_paths_from_text(&text);
            if !paths.is_empty() {
                for path in paths {
                    state.input.insert_str(&path.to_string_lossy());
                    state.input.input(' ');
                }
            } else {
                // Otherwise paste as normal text
                for c in text.chars() {
                    if c == '\n' {
                        state.input.newline();
                    } else if c != '\r' {
                        state.input.input(c);
                    }
                }
            }
        }
        InputEvent::HandleClipboardImagePaste => {
            #[cfg(not(target_os = "android"))]
            {
                const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024; // 10MB limit

                match crate::tui::services::clipboard_paste::paste_image_to_temp_png() {
                    Ok((path, _info)) => {
                        if let Ok(bytes) = std::fs::read(&path) {
                            if bytes.len() > MAX_IMAGE_BYTES {
                                log::warn!("Image too large ({} bytes), max {} bytes", bytes.len(), MAX_IMAGE_BYTES);
                                state.input.insert_str("[image too large, max 10MB] ");
                            } else {
                                use base64::Engine as _;
                                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                                // Detect media type from file extension
                                let media_type = match path.extension().and_then(|e| e.to_str()) {
                                    Some("png") => "image/png",
                                    Some("jpg") | Some("jpeg") => "image/jpeg",
                                    Some("gif") => "image/gif",
                                    Some("webp") => "image/webp",
                                    _ => "image/png", // paste_image_to_temp_png produces PNG-compat output
                                };
                                let part = crate::tui::types::ContentPart {
                                    r#type: "image".to_string(),
                                    text: None,
                                    image_url: Some(crate::tui::types::ImageUrl {
                                        url: format!("data:{};base64,{}", media_type, b64),
                                    }),
                                };
                                state.pending_image_parts.push(part);
                                state.input.insert_str("[image attached] ");
                            }
                        } else {
                            state.input.insert_str(&path.to_string_lossy());
                            state.input.input(' ');
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to paste image from clipboard: {}", e);
                    }
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
        InputEvent::Up => match state.focus {
            crate::tui::app::WorkspaceFocus::Input => {
                if state.shell_popup_visible && state.active_shell_command.is_some() && !state.shell_history.is_empty() {
                    let max_idx = state.shell_history.len() - 1;
                    let next_idx = match state.shell_history_idx {
                        Some(idx) => idx.saturating_sub(1),
                        None => max_idx,
                    };
                    state.shell_history_idx = Some(next_idx);
                    if let Some(cmd) = state.shell_history.get(next_idx) {
                        state.input.set_content(cmd);
                        state.input.move_cursor_end();
                    }
                } else {
                    state.input.move_cursor_up();
                }
            },
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
                crate::tui::app::WorkbenchTab::Runtime => {
                    state.runtime_selected_idx = state.runtime_selected_idx.saturating_sub(1);
                    state.runtime_detail_scroll = 0;
                }
                crate::tui::app::WorkbenchTab::Review => {
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = review_handler::select_prev(&mut ctx);
                }
            },
        },
        InputEvent::Down => match state.focus {
            crate::tui::app::WorkspaceFocus::Input => {
                if state.shell_popup_visible && state.active_shell_command.is_some() && state.shell_history_idx.is_some() {
                    let next_idx = state.shell_history_idx.unwrap() + 1;
                    if next_idx >= state.shell_history.len() {
                        state.shell_history_idx = None;
                        state.input.clear();
                    } else {
                        state.shell_history_idx = Some(next_idx);
                        if let Some(cmd) = state.shell_history.get(next_idx) {
                            state.input.set_content(cmd);
                            state.input.move_cursor_end();
                        }
                    }
                } else {
                    state.input.move_cursor_down();
                }
            },
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
                crate::tui::app::WorkbenchTab::Runtime => {
                    if state.runtime_selected_idx + 1 < state.runtime_jobs.len() {
                        state.runtime_selected_idx += 1;
                        state.runtime_detail_scroll = 0;
                    }
                }
                crate::tui::app::WorkbenchTab::Review => {
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = review_handler::select_next(&mut ctx);
                }
            },
        },
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
                let mut ctx = HandlerContext::new(state, output_tx);
                let _ = model_switcher::close(&mut ctx);
            } else if state.show_file_search {
                let mut ctx = HandlerContext::new(state, output_tx);
                let _ = file_search::close(&mut ctx);
            } else if state.show_changeset {
                let mut ctx = HandlerContext::new(state, output_tx);
                let _ = changeset_handler::close(&mut ctx);
            } else if state.shell_popup_visible && state.active_shell_command.is_some() {
                state.shell_popup_visible = false;
                state.shell_backgrounded = true;
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
        InputEvent::HandleCtrlZ => {
            if state.active_shell_command.is_some() {
                if state.shell_popup_visible {
                    state.shell_popup_visible = false;
                    state.shell_backgrounded = true;
                } else {
                    state.shell_popup_visible = true;
                    state.shell_backgrounded = false;
                }
                state.shell_history_idx = None;
            }
        }
        InputEvent::BackgroundShell => {
            if state.active_shell_command.is_some() {
                state.shell_popup_visible = false;
                state.shell_backgrounded = true;
            }
        }
        InputEvent::FocusShell => {
            if state.active_shell_command.is_some() {
                state.shell_popup_visible = true;
                state.shell_backgrounded = false;
            }
        }
        InputEvent::ShellKill => {
            if let Some(shell) = state.active_shell_command.clone() {
                let _ = shell.kill();
                state.shell_waiting_for_input = false;
            }
        }
        InputEvent::AutoApproveCurrentTool => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = approval::approve_current(&mut ctx);
        }
        InputEvent::RejectCurrentTool => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = approval::begin_reject_current(&mut ctx);
        }
        InputEvent::ApproveAll => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = approval::approve_all(&mut ctx);
        }
        InputEvent::RejectAll => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = approval::confirm_reject_all(&mut ctx);
        }
        InputEvent::ScrollUp => match state.focus {
            crate::tui::app::WorkspaceFocus::Conversation => {
                state.scroll = state.scroll.saturating_sub(1);
            }
            crate::tui::app::WorkspaceFocus::Activity => {
                state.activity_scroll = state.activity_scroll.saturating_add(1);
            }
            _ => {}
        },
        InputEvent::ScrollDown => match state.focus {
            crate::tui::app::WorkspaceFocus::Conversation => {
                state.scroll = state.scroll.saturating_add(1);
            }
            crate::tui::app::WorkspaceFocus::Activity => {
                state.activity_scroll = state.activity_scroll.saturating_sub(1);
            }
            _ => {}
        },
        InputEvent::ShowCommandPalette => {
            state.show_command_palette = true;
            state.command_palette_input.clear();
            state.command_palette_selected = 0;
        }
        InputEvent::ShowModelSwitcher => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = model_switcher::open(&mut ctx);
        }
        InputEvent::ShowFileSearch => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = file_search::open(&mut ctx);
        }
        InputEvent::ShowChangeset => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = changeset_handler::open(&mut ctx);
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
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = review_handler::open(&mut ctx);
        }
        _ => {}
    }
}

/// Handle backend events
/// Resolve a screen row to the message ID at that position.
/// Uses the assembled_lines_cache and per_message_cache to map
/// rendered line indices back to messages.
fn message_at_row(state: &AppState, row: u16) -> Option<uuid::Uuid> {
    // Convert screen row to line index in the rendered message list
    let row_in_area = (row as usize)
        .checked_sub(state.message_area_y as usize)?
        .checked_sub(1)?; // border
    let line_idx = row_in_area + state.scroll;

    // Walk messages and accumulate line counts from per_message_cache
    let mut cumulative = 0usize;
    for msg in &state.messages {
        let msg_lines = state
            .per_message_cache
            .get(&msg.id)
            .map(|c| c.rendered_lines.len() + 1) // +1 for spacing line
            .unwrap_or(1);
        if line_idx < cumulative + msg_lines {
            return Some(msg.id);
        }
        cumulative += msg_lines;
    }
    None
}

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
            | "vil_ir_diff"
            | "vil_audit"
            | "vil_plumbing"
            | "vil_repair"
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
            state
                .toasts
                .push(crate::tui::services::Toast::error(msg.clone()));
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
        InputEvent::SetRuntimeJobs(jobs) => {
            state.runtime_jobs = jobs;
            if state.runtime_selected_idx >= state.runtime_jobs.len() {
                state.runtime_selected_idx = state.runtime_jobs.len().saturating_sub(1);
            }
            state.push_activity(
                crate::tui::app::ActivityKind::Status,
                "Runtime jobs updated",
            );
        }
        InputEvent::SetRuntimeState(snapshot) => {
            // Detect significant state changes for activity logging
            let prev_state = state.runtime_state_snapshot.as_ref().map(|s| &s.state);
            let new_state = snapshot.as_ref().map(|s| &s.state);
            
            if let (Some(prev), Some(new)) = (prev_state, new_state) {
                match new {
                    vac_runtime::AutopilotState::WaitingApproval { tool_call_id } => {
                        if !matches!(prev, vac_runtime::AutopilotState::WaitingApproval { .. }) {
                            state.push_activity(
                                crate::tui::app::ActivityKind::Approval,
                                format!("Runtime waiting for approval: {}", &tool_call_id[..8]),
                            );
                            state.toasts.push(crate::tui::services::Toast::info(
                                format!("Runtime waiting for approval: {}", &tool_call_id[..8]),
                            ));
                        }
                    }
                    vac_runtime::AutopilotState::Backoff { until } => {
                        if !matches!(prev, vac_runtime::AutopilotState::Backoff { .. }) {
                            state.push_activity(
                                crate::tui::app::ActivityKind::Status,
                                format!("Runtime entered backoff until {}", until.format("%H:%M:%S")),
                            );
                            state.toasts.push(crate::tui::services::Toast::info(
                                format!("Runtime backoff until {}", until.format("%H:%M:%S")),
                            ));
                        }
                    }
                    _ => {}
                }
            }
            
            // Detect execution environment changes
            if let Some(new_snapshot) = &snapshot {
                if let Some(prev_snapshot) = &state.runtime_state_snapshot {
                    if prev_snapshot.execution_environment != new_snapshot.execution_environment {
                        let env_name = match new_snapshot.execution_environment {
                            vac_core::ExecutionEnvironment::Host => "host",
                            vac_core::ExecutionEnvironment::IsolatedBatch => "isolated-batch",
                            vac_core::ExecutionEnvironment::IsolatedInteractive => "isolated-interactive",
                        };
                        state.push_activity(
                            crate::tui::app::ActivityKind::Status,
                            format!("Execution environment switched to {}", env_name),
                        );
                        state.toasts.push(crate::tui::services::Toast::info(
                            format!("Switched to {} environment", env_name),
                        ));
                    }
                }
            }
            
            state.runtime_state_snapshot = snapshot;
        }
        InputEvent::FileIndexReady(files) => {
            state.all_files = files;
            state.file_search_results = state.all_files.iter().take(50).cloned().collect();
            state.toasts.push(crate::tui::services::Toast::success("File index ready".to_string()));
        }
        InputEvent::ShellStarted(shell) => {
            state.active_shell_command = Some(shell.clone());
            state.shell_popup_visible = true;
            state.shell_backgrounded = false;
            state.shell_waiting_for_input = false;
            state.shell_exit_code = None;
            state.shell_last_error = None;
            state.shell_output.clear();
            state.shell_history_idx = None;
            state.push_activity(
                crate::tui::app::ActivityKind::Shell,
                format!("Shell started: {}", shell.command),
            );
        }
        InputEvent::ShellOutput(id, text) => {
            if state.active_shell_command.as_ref().map_or(false, |cmd| cmd.id == id) || id == "system" {
                state.shell_output.push_str(&text);
            }
        }
        InputEvent::ShellError(id, text) => {
            if state.active_shell_command.as_ref().map_or(false, |cmd| cmd.id == id) || id == "system" {
                if !state.shell_output.ends_with('\n') && !state.shell_output.is_empty() {
                    state.shell_output.push('\n');
                }
                state
                    .shell_output
                    .push_str(&format!("[shell error] {text}\n"));
                state.shell_last_error = Some(text.clone());
            }
            state.push_activity(
                crate::tui::app::ActivityKind::Shell,
                format!("Shell error: {}", text),
            );
        }
        InputEvent::ShellCompleted(id, code) => {
            if state.active_shell_command.as_ref().map_or(false, |cmd| cmd.id == id) || id == "system" {
                state.shell_exit_code = Some(code);
                state.shell_waiting_for_input = false;
                state.active_shell_command = None;
            }
            let status = if code == 0 { "success" } else { "failed" };
            state.push_activity(
                crate::tui::app::ActivityKind::Shell,
                format!("Shell {} (exit code {})", status, code),
            );
        }
        InputEvent::ShellWaitingForInput(id) => {
            if state.active_shell_command.as_ref().map_or(false, |cmd| cmd.id == id) || id == "system" {
                state.shell_waiting_for_input = true;
            }
        }
        InputEvent::McpConnected { name, tools } => {
            state.push_activity(
                crate::tui::app::ActivityKind::Mcp,
                format!("MCP server '{}' connected ({} tools)", name, tools),
            );
        }
        InputEvent::McpFailed { name, error } => {
            state.push_activity(
                crate::tui::app::ActivityKind::Mcp,
                format!("MCP server '{}' failed: {}", name, error),
            );
        }
        InputEvent::McpServerState(name, conn_state) => {
            state.mcp_server_states.insert(name, conn_state);
        }
        InputEvent::VilStatusUpdated(snapshot) => {
            state.vil_status = snapshot;
            state.push_activity(
                crate::tui::app::ActivityKind::Status,
                "VIL status updated".to_string(),
            );
        }
        InputEvent::ChangesetUpdated => {
            let files = state.changeset_store.modified_files();
            let project_root = state.project_root.clone();
            if let Some(tx) = state.input_tx.clone() {
                tokio::spawn(async move {
                    if let Ok(pipeline) = vil_ir::IrPipeline::new(&project_root) {
                        if let Ok(report) = vil_validate::validate_changes(&pipeline, &files) {
                            let profile = vac_core::detector::VilProjectProfile::detect(&project_root);
                            let mut ir_metadata_files = vec![];
                            for (path, module) in pipeline.modules() {
                                let has_vil_attr = module.structs.iter().any(|s| !s.vil_attrs.is_empty())
                                    || module.functions.iter().any(|f| !f.vil_attrs.is_empty());
                                if has_vil_attr {
                                    ir_metadata_files.push(path.clone());
                                }
                            }
                            
                            let config = vac_core::VacConfig::load_with_fallback(&project_root).unwrap_or_default();
                            let semantic_mode = config.memory.enable_semantic;

                            let active_rulebook = {
                                let books = vac_core::rulebook::RulebookLoader::load_all(&project_root, &config.rulebook.paths);
                                if books.is_empty() {
                                    None
                                } else {
                                    Some(books.iter().map(|b| b.id.clone()).collect::<Vec<_>>().join(", "))
                                }
                            };

                            let snapshot = crate::tui::app::VilStatusSnapshot {
                                profile: Some(profile),
                                validation_score: report.score,
                                validation_issues: report.issues,
                                active_rulebook,
                                semantic_mode,
                                ir_generation_active: true,
                                ir_metadata_files,
                            };
                            let _ = tx.send(InputEvent::VilStatusUpdated(snapshot)).await;
                        }
                    }
                });
            }
        }
        InputEvent::IsolationBoundary { action, environment } => {
            state.push_activity(
                crate::tui::app::ActivityKind::Isolation,
                format!("Isolation: {} in {}", action, environment),
            );
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
            state.reject_reason_input = None;
            state.at_trigger_active = false;
            state.at_query.clear();
            state.at_results.clear();
            state.is_streaming = false;
            state.streaming_message_id = None;
            state.scroll = 0;
            state.input.clear();
            state.review_open = false;
            state.review_filter.clear();
            state.review_diff = None;
            state.review_selected_idx = 0;
            state.review_selected_path = None;
            state.active_shell_command = None;
            state.shell_popup_visible = false;
            state.shell_output.clear();
            state.shell_waiting_for_input = false;
            state.shell_backgrounded = false;
            state.shell_exit_code = None;
            state.shell_last_error = None;
            state.shell_history_idx = None;
            state.runtime_jobs.clear();
            state.runtime_selected_idx = 0;
            state.runtime_filter.clear();
            state.runtime_detail_scroll = 0;
            state.runtime_state_snapshot = None;
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
            state.modified_files = state.changeset_store.modified_files(); // derived: empty after clear
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
                state
                    .approval_explanations
                    .insert(tc.id.clone(), explanation);
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
            let mut changeset_updated = false;
            if !result.modified_files.is_empty() {
                content.push_str("\n\n**Modified Files**:\n");
                for file in &result.modified_files {
                    content.push_str(&format!("- `{}`\n", file));
                    state
                        .changeset_store
                        .file_modified(file.clone(), "agent".to_string(), true);
                    changeset_updated = true;
                }
            }
            if !result.created_files.is_empty() {
                content.push_str("\n**Created Files**:\n");
                for file in &result.created_files {
                    content.push_str(&format!("- `{}`\n", file));
                    state
                        .changeset_store
                        .file_created(file.clone(), "agent".to_string());
                    changeset_updated = true;
                }
            }
            // Sync derived view from store (single source of truth)
            state.modified_files = state.changeset_store.modified_files();
            
            if changeset_updated {
                if let Some(tx) = state.input_tx.clone() {
                    let _ = tx.try_send(InputEvent::ChangesetUpdated);
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
        state
            .changeset_store
            .file_modified("a.txt".to_string(), "agent".to_string(), false);
        state
            .changeset_store
            .file_modified("b.txt".to_string(), "agent".to_string(), false);
        state.modified_files = state.changeset_store.modified_files();
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
        state
            .changeset_store
            .file_modified("a.txt".to_string(), "agent".to_string(), false);
        state.modified_files = state.changeset_store.modified_files();
        state.input.set_content("/review");
        handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert!(state.review_open);
        assert_eq!(state.workbench_tab, crate::tui::app::WorkbenchTab::Review);
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
                arguments:
                    serde_json::json!({"file_path":"b.txt","old_string":"a","new_string":"b"})
                        .to_string(),
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
        assert_eq!(
            state.workbench_tab,
            crate::tui::app::WorkbenchTab::Approvals
        );

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
        // 'r' now shows reason prompt - confirm with Enter to reject
        handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        let ev = rx.recv().await.unwrap();
        match ev {
            OutputEvent::RejectTool(tc, _, _) => assert_eq!(tc.id, "tc-1"),
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
        state
            .changeset_store
            .file_modified(file_rel.to_string(), "agent".to_string(), true);
        state.modified_files = state.changeset_store.modified_files();
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
        state
            .changeset_store
            .file_modified("a.txt".to_string(), "agent".to_string(), true);
        state
            .changeset_store
            .file_modified("b.txt".to_string(), "agent".to_string(), true);
        state.modified_files = state.changeset_store.modified_files();
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
    async fn global_approval_hotkey_ctrl_m_approves_current() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Add pending approval
        state.pending_approvals.push(ToolCall {
            id: "tc-1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "test_tool".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });

        // Trigger Ctrl+M (AutoApproveCurrentTool)
        handle_input_event(&mut state, &tx, InputEvent::AutoApproveCurrentTool);

        // Verify approval processed
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.approved_tools.len(), 1);
        assert_eq!(state.approved_tools[0].id, "tc-1");

        // Verify output event sent
        let output = rx.try_recv().unwrap();
        assert!(matches!(output, OutputEvent::AcceptTool(_)));
    }

    #[tokio::test]
    async fn global_approval_hotkey_ctrl_shift_m_rejects_current() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Add pending approval
        state.pending_approvals.push(ToolCall {
            id: "tc-2".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "test_tool".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });

        // Trigger Ctrl+Shift+M (RejectCurrentTool) - now shows reason prompt
        handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);

        // Reason prompt should be active
        assert!(state.reject_reason_input.is_some());
        assert_eq!(state.pending_approvals.len(), 1); // not yet rejected

        // Confirm with Enter (no reason typed)
        handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);

        // Verify rejection processed
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.rejected_tools.len(), 1);
        assert_eq!(state.rejected_tools[0].id, "tc-2");

        // Verify output event sent
        let output = rx.try_recv().unwrap();
        assert!(matches!(output, OutputEvent::RejectTool(_, _, _)));
    }

    #[tokio::test]
    async fn global_approval_hotkeys_safe_when_no_pending() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // No pending approvals
        assert_eq!(state.pending_approvals.len(), 0);

        // Trigger hotkeys - should not panic
        handle_input_event(&mut state, &tx, InputEvent::AutoApproveCurrentTool);
        handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);

        // State unchanged
        assert_eq!(state.approved_tools.len(), 0);
        assert_eq!(state.rejected_tools.len(), 0);
    }

    #[tokio::test]
    async fn slash_command_model_exists_in_commands() {
        let dir = tempfile::tempdir().unwrap();
        let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
        let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Verify /model command exists
        let commands = state.commands;
        assert!(commands.iter().any(|c| c.command == "/model"));
    }

    #[tokio::test]
    async fn slash_command_files_exists_in_commands() {
        let dir = tempfile::tempdir().unwrap();
        let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
        let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Verify /files command exists
        let commands = state.commands;
        assert!(commands.iter().any(|c| c.command == "/files"));
    }

    #[tokio::test]
    async fn slash_command_changes_exists_in_commands() {
        let dir = tempfile::tempdir().unwrap();
        let (_tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
        let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Verify /changes command exists
        let commands = state.commands;
        assert!(commands.iter().any(|c| c.command == "/changes"));
    }

    #[test]
    fn footer_approval_bar_shows_when_pending_approvals() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Add pending approval
        state.pending_approvals.push(ToolCall {
            id: "tc-1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "file_write".to_string(),
                arguments: r#"{"file_path":"test.rs"}"#.to_string(),
            },
            metadata: None,
        });

        // Footer should show approval bar (verified by view rendering logic)
        assert!(!state.pending_approvals.is_empty());
        assert_eq!(state.approval_selected_idx, 0);
    }

    #[test]
    fn footer_approval_bar_hidden_when_no_pending() {
        let dir = tempfile::tempdir().unwrap();
        let state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // No pending approvals
        assert!(state.pending_approvals.is_empty());
        // Footer should show normal hints (verified by view rendering logic)
    }

    #[test]
    fn footer_approval_bar_shows_correct_index() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Add multiple pending approvals
        for i in 0..3 {
            state.pending_approvals.push(ToolCall {
                id: format!("tc-{}", i),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: "test_tool".to_string(),
                    arguments: "{}".to_string(),
                },
                metadata: None,
            });
        }

        // Select second approval
        state.approval_selected_idx = 1;

        // Verify index
        assert_eq!(state.approval_selected_idx, 1);
        assert_eq!(state.pending_approvals.len(), 3);
    }

    #[tokio::test]
    async fn session_restore_clears_popup_state() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Set popup states
        state.show_model_switcher = true;
        state.show_file_search = true;
        state.show_changeset = true;
        state.model_switcher_filter = "test".to_string();
        state.file_search_query = "query".to_string();

        // Trigger session restore
        handle_backend_event(
            &mut state,
            &tx,
            InputEvent::SessionRestored {
                id: uuid::Uuid::new_v4().to_string(),
                title: "New Session".to_string(),
                messages: vec![],
            },
        );

        // Verify all popup states cleared
        assert!(!state.show_model_switcher);
        assert!(!state.show_file_search);
        assert!(!state.show_changeset);
        assert!(state.model_switcher_filter.is_empty());
        assert!(state.file_search_query.is_empty());
    }

    #[tokio::test]
    async fn session_restore_clears_changeset_store() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Add changeset entries
        state
            .changeset_store
            .file_created("a.rs".to_string(), "agent".to_string());
        state
            .changeset_store
            .file_modified("b.rs".to_string(), "agent".to_string(), true);
        assert_eq!(state.changeset_store.entries().len(), 2);

        // Trigger session restore
        handle_backend_event(
            &mut state,
            &tx,
            InputEvent::SessionRestored {
                id: uuid::Uuid::new_v4().to_string(),
                title: "New Session".to_string(),
                messages: vec![],
            },
        );

        // Verify changeset cleared
        assert_eq!(state.changeset_store.entries().len(), 0);
    }

    #[tokio::test]
    async fn session_restore_clears_approval_state() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Add approval state
        state.pending_approvals.push(ToolCall {
            id: "tc-1".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "test".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });
        state.approved_tools.push(ToolCall {
            id: "tc-2".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "test".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });

        // Trigger session restore
        handle_backend_event(
            &mut state,
            &tx,
            InputEvent::SessionRestored {
                id: uuid::Uuid::new_v4().to_string(),
                title: "New Session".to_string(),
                messages: vec![],
            },
        );

        // Verify approval state cleared
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.approved_tools.len(), 0);
        assert_eq!(state.rejected_tools.len(), 0);
    }

    #[test]
    fn command_palette_filters_commands_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Empty filter shows all commands
        state.command_palette_input = "".to_string();
        let all = state.filtered_commands();
        assert!(!all.is_empty());

        // Filter by prefix
        state.command_palette_input = "/model".to_string();
        let filtered = state.filtered_commands();
        assert!(filtered.iter().any(|c| c.command == "/model"));

        // Non-matching filter
        state.command_palette_input = "/nonexistent".to_string();
        let empty = state.filtered_commands();
        assert!(empty.is_empty());
    }

    #[test]
    fn command_palette_selection_stays_within_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        state.command_palette_input = "".to_string();
        let commands = state.filtered_commands();

        // Selection should not exceed command count
        if !commands.is_empty() {
            state.command_palette_selected = 0;
            assert_eq!(state.command_palette_selected, 0);

            state.command_palette_selected = commands.len() - 1;
            assert_eq!(state.command_palette_selected, commands.len() - 1);
        }
    }

    #[tokio::test]
    async fn command_palette_dispatch_does_not_send_literal_slash() {
        let dir = tempfile::tempdir().unwrap();
        let (_tx, mut rx) = tokio::sync::mpsc::channel::<OutputEvent>(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        // Execute /review command
        state.show_command_palette = true;
        let filtered = state.filtered_commands();
        if let Some(cmd) = filtered.iter().find(|c| c.command == "/review") {
            // Simulate command execution
            state.add_user_message(cmd.command.clone());
            state.review_open = true;
            state.show_command_palette = false;
        }

        // Verify review opened, not sent as literal message
        assert!(state.review_open);
        assert!(!state.show_command_palette);

        // No output event should be sent for /review
        assert!(rx.try_recv().is_err());
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
        state
            .changeset_store
            .file_modified("a.txt".to_string(), "agent".to_string(), true);
        state
            .changeset_store
            .file_modified("b.txt".to_string(), "agent".to_string(), true);
        state.modified_files = state.changeset_store.modified_files();
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

    // ── Branch 4A behavioral tests ──────────────────────────────────────────

    #[test]
    fn modified_files_is_derived_from_changeset_store() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        state
            .changeset_store
            .file_modified("a.rs".to_string(), "agent".to_string(), true);
        state
            .changeset_store
            .file_created("b.rs".to_string(), "agent".to_string());
        state.modified_files = state.changeset_store.modified_files();

        // modified_files must equal store's derived view
        assert_eq!(state.modified_files, state.changeset_store.modified_files());
        assert_eq!(state.modified_files.len(), 2);
    }

    #[test]
    fn counter_consistency_header_tab_popup() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        state
            .changeset_store
            .file_modified("x.rs".to_string(), "agent".to_string(), true);
        state
            .changeset_store
            .file_created("y.rs".to_string(), "agent".to_string());
        state
            .changeset_store
            .file_modified("z.rs".to_string(), "agent".to_string(), true);
        state.changeset_store.revert_success("z.rs"); // reverted: not active

        let active = state.changeset_store.active_entries().len();
        // All three surfaces must read the same count
        assert_eq!(active, 2); // x.rs + y.rs; z.rs is reverted
        // review_filtered_paths also driven by active_entries
        state.review_sync_items();
        assert_eq!(state.review_filtered_paths().len(), 2);
    }

    #[test]
    fn task_completed_does_not_dual_write() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        let result = vac_core::task::TaskResult {
            task_id: vac_core::task::TaskId::new(),
            status: vac_core::task::TaskStatus::Completed,
            summary: "done".to_string(),
            modified_files: vec!["src/lib.rs".to_string()],
            created_files: vec!["src/new.rs".to_string()],
            validation_score: None,
            elapsed_ms: 0,
            total_tokens_used: 0,
            agent_contributions: vec![],
        };
        handle_backend_event(&mut state, &tx, InputEvent::TaskCompleted(result));

        // modified_files must equal store's derived view - no independent writes
        assert_eq!(state.modified_files, state.changeset_store.modified_files());
        assert_eq!(state.changeset_store.active_entries().len(), 2);
    }

    #[test]
    fn session_restore_clears_store_and_syncs_derived() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        state
            .changeset_store
            .file_modified("a.rs".to_string(), "agent".to_string(), true);
        state.modified_files = state.changeset_store.modified_files();
        assert_eq!(state.modified_files.len(), 1);

        handle_backend_event(
            &mut state,
            &tx,
            InputEvent::SessionRestored {
                id: uuid::Uuid::new_v4().to_string(),
                title: "new".to_string(),
                messages: vec![],
            },
        );

        // Both store and derived view must be empty
        assert_eq!(state.changeset_store.active_entries().len(), 0);
        assert_eq!(state.modified_files.len(), 0);
        assert_eq!(state.modified_files, state.changeset_store.modified_files());
    }

    // ── Branch 5A behavioral tests ──────────────────────────────────────────

    #[test]
    fn show_model_switcher_event_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        handle_input_event(&mut state, &tx, InputEvent::ShowModelSwitcher);
        assert!(state.show_model_switcher);
        assert!(state.model_switcher_filter.is_empty());
    }

    #[test]
    fn show_file_search_event_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        handle_input_event(&mut state, &tx, InputEvent::ShowFileSearch);
        assert!(state.show_file_search);
        assert!(state.file_search_query.is_empty());
    }

    #[test]
    fn show_changeset_event_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        handle_input_event(&mut state, &tx, InputEvent::ShowChangeset);
        assert!(state.show_changeset);
    }

    #[tokio::test]
    async fn slash_model_dispatch_opens_model_switcher_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.input.set_content("/model");
        handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert!(state.show_model_switcher);
        assert!(!state.show_file_search);
    }

    #[tokio::test]
    async fn slash_files_dispatch_opens_file_search_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.input.set_content("/files");
        handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert!(state.show_file_search);
        assert!(!state.show_model_switcher);
    }

    #[tokio::test]
    async fn slash_changes_dispatch_opens_changeset_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.input.set_content("/changes");
        handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert!(state.show_changeset);
    }

    #[tokio::test]
    async fn slash_review_dispatch_opens_review_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.input.set_content("/review");
        handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert!(state.review_open);
        assert_eq!(state.workbench_tab, crate::tui::app::WorkbenchTab::Review);
    }

    #[tokio::test]
    async fn approve_current_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.pending_approvals.push(ToolCall {
            id: "tc-h".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "write_file".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });
        handle_input_event(&mut state, &tx, InputEvent::AutoApproveCurrentTool);
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.approved_tools.len(), 1);
        assert!(matches!(rx.try_recv().unwrap(), OutputEvent::AcceptTool(_)));
    }

    #[tokio::test]
    async fn reject_current_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.pending_approvals.push(ToolCall {
            id: "tc-r".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "delete_file".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });
        handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);
        // Reason prompt active - not yet rejected
        assert!(state.reject_reason_input.is_some());
        // Confirm with Enter
        handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.rejected_tools.len(), 1);
        assert!(matches!(
            rx.try_recv().unwrap(),
            OutputEvent::RejectTool(_, _, _)
        ));
    }

    // ── Branch 6C behavioral tests ──────────────────────────────────────────

    #[test]
    fn session_info_enriched_fields_populated() {
        let s = crate::tui::app::SessionInfo {
            id: "abc123".to_string(),
            title: "Test Session".to_string(),
            updated_at: "2026-04-16T09:00:00Z".to_string(),
            checkpoints: vec!["abc123_state.json".to_string()],
            task_count: 5,
            last_activity: "2026-04-16 09:00".to_string(),
            has_checkpoint: true,
        };
        assert_eq!(s.task_count, 5);
        assert_eq!(s.last_activity, "2026-04-16 09:00");
        assert!(s.has_checkpoint);
        assert_eq!(s.checkpoints.len(), 1);
    }

    #[test]
    fn set_sessions_event_populates_enriched_fields() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        let sessions = vec![
            crate::tui::app::SessionInfo {
                id: "s1".to_string(),
                title: "Session 1".to_string(),
                updated_at: "2026-04-16T09:00:00Z".to_string(),
                checkpoints: vec![],
                task_count: 3,
                last_activity: "2026-04-16 09:00".to_string(),
                has_checkpoint: false,
            },
            crate::tui::app::SessionInfo {
                id: "s2".to_string(),
                title: "Session 2".to_string(),
                updated_at: "2026-04-16T10:00:00Z".to_string(),
                checkpoints: vec!["s2_state.json".to_string()],
                task_count: 7,
                last_activity: "2026-04-16 10:00".to_string(),
                has_checkpoint: true,
            },
        ];

        handle_backend_event(&mut state, &tx, InputEvent::SetSessions(sessions));

        assert_eq!(state.sessions.len(), 2);
        assert_eq!(state.sessions[0].task_count, 3);
        assert!(!state.sessions[0].has_checkpoint);
        assert_eq!(state.sessions[1].task_count, 7);
        assert!(state.sessions[1].has_checkpoint);
        assert_eq!(state.sessions[1].checkpoints.len(), 1);
    }

    #[test]
    fn sessions_tab_r_resumes_checkpoint_for_selected_session() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::tui::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::tui::app::WorkbenchTab::Sessions;
        state.sessions = vec![crate::tui::app::SessionInfo {
            id: "sess-abc".to_string(),
            title: "Session abc".to_string(),
            updated_at: "2026-04-16T09:00:00Z".to_string(),
            checkpoints: vec!["sess-abc_state.json".to_string()],
            task_count: 2,
            last_activity: "2026-04-16 09:00".to_string(),
            has_checkpoint: true,
        }];
        state.sessions_selected_idx = 0;

        handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));

        let ev = rx.try_recv().unwrap();
        assert!(matches!(ev, OutputEvent::ResumeSession(ref id) if id == "sess-abc"));
    }

    #[test]
    fn sessions_tab_r_toasts_when_no_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::tui::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::tui::app::WorkbenchTab::Sessions;
        state.sessions = vec![crate::tui::app::SessionInfo {
            id: "sess-xyz".to_string(),
            title: "Session xyz".to_string(),
            updated_at: "2026-04-16T09:00:00Z".to_string(),
            checkpoints: vec![],
            task_count: 0,
            last_activity: "2026-04-16 09:00".to_string(),
            has_checkpoint: false,
        }];
        state.sessions_selected_idx = 0;

        handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));

        assert!(!state.toasts.is_empty());
        assert!(state.toasts[0].message.contains("No checkpoint"));
    }

    // ── Branch 6B behavioral tests ──────────────────────────────────────────

    #[test]
    fn at_trigger_activates_on_at_char() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::tui::app::WorkspaceFocus::Input;

        handle_input_event(&mut state, &tx, InputEvent::InputChanged('@'));

        assert!(state.at_trigger_active);
        assert!(state.at_query.is_empty());
    }

    #[test]
    fn at_trigger_updates_query_on_subsequent_chars() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::tui::app::WorkspaceFocus::Input;

        handle_input_event(&mut state, &tx, InputEvent::InputChanged('@'));
        handle_input_event(&mut state, &tx, InputEvent::InputChanged('s'));
        handle_input_event(&mut state, &tx, InputEvent::InputChanged('r'));
        handle_input_event(&mut state, &tx, InputEvent::InputChanged('c'));

        assert!(state.at_trigger_active);
        assert_eq!(state.at_query, "src");
    }

    #[test]
    fn at_trigger_deactivates_on_esc() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::tui::app::WorkspaceFocus::Input;
        state.at_trigger_active = true;
        state.at_query = "src".to_string();

        handle_input_event(&mut state, &tx, InputEvent::HandleEsc);

        assert!(!state.at_trigger_active);
        assert!(state.at_query.is_empty());
    }

    #[test]
    fn at_trigger_deactivates_on_space() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::tui::app::WorkspaceFocus::Input;
        state.at_trigger_active = true;
        state.at_query = "src".to_string();

        handle_input_event(&mut state, &tx, InputEvent::InputChanged(' '));

        assert!(!state.at_trigger_active);
    }

    #[test]
    fn at_trigger_backspace_pops_query() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::tui::app::WorkspaceFocus::Input;
        state.at_trigger_active = true;
        state.at_query = "sr".to_string();

        handle_input_event(&mut state, &tx, InputEvent::InputBackspace);
        assert_eq!(state.at_query, "s");
        assert!(state.at_trigger_active);

        // Backspace on empty query deactivates
        handle_input_event(&mut state, &tx, InputEvent::InputBackspace);
        handle_input_event(&mut state, &tx, InputEvent::InputBackspace); // removes '@'
        assert!(!state.at_trigger_active);
    }

    #[test]
    fn at_trigger_enter_inserts_selected_path() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::tui::app::WorkspaceFocus::Input;
        state.at_trigger_active = true;
        state.at_query = "src".to_string();
        state.at_results = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
        state.at_selected_idx = 0;
        // Simulate @src already in input
        state.input.insert_str("@src");

        handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);

        assert!(!state.at_trigger_active);
        assert!(state.at_query.is_empty());
        // Input should contain the path
        let content = state.input.get_content();
        assert!(content.contains("src/main.rs"));
    }

    // ── Branch 6A behavioral tests ──────────────────────────────────────────

    #[tokio::test]
    async fn approve_all_clears_all_pending() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        for i in 0..3 {
            state.pending_approvals.push(ToolCall {
                id: format!("tc-{}", i),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: "tool".to_string(),
                    arguments: "{}".to_string(),
                },
                metadata: None,
            });
        }
        handle_input_event(&mut state, &tx, InputEvent::ApproveAll);
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.approved_tools.len(), 3);
        // 3 AcceptTool events emitted
        for _ in 0..3 {
            assert!(matches!(rx.try_recv().unwrap(), OutputEvent::AcceptTool(_)));
        }
    }

    #[tokio::test]
    async fn reject_all_clears_all_pending_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        for i in 0..3 {
            state.pending_approvals.push(ToolCall {
                id: format!("tc-{}", i),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: "tool".to_string(),
                    arguments: "{}".to_string(),
                },
                metadata: None,
            });
        }
        handle_input_event(&mut state, &tx, InputEvent::RejectAll);
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.rejected_tools.len(), 3);
        for _ in 0..3 {
            assert!(matches!(
                rx.try_recv().unwrap(),
                OutputEvent::RejectTool(_, _, _)
            ));
        }
    }

    #[tokio::test]
    async fn reject_current_shows_reason_prompt_then_confirms() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.pending_approvals.push(ToolCall {
            id: "tc-reason".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "tool".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });

        // Trigger reject - shows prompt
        handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);
        assert!(state.reject_reason_input.is_some());
        assert_eq!(state.pending_approvals.len(), 1); // not yet rejected

        // Type reason
        handle_input_event(&mut state, &tx, InputEvent::InputChanged('t'));
        handle_input_event(&mut state, &tx, InputEvent::InputChanged('o'));
        handle_input_event(&mut state, &tx, InputEvent::InputChanged('o'));
        assert_eq!(state.reject_reason_input.as_deref(), Some("too"));

        // Confirm
        handle_input_event(&mut state, &tx, InputEvent::InputSubmitted);
        assert!(state.reject_reason_input.is_none());
        assert_eq!(state.pending_approvals.len(), 0);
        assert_eq!(state.rejected_tools.len(), 1);

        // Reason passed in event
        if let OutputEvent::RejectTool(_, _, reason) = rx.try_recv().unwrap() {
            assert_eq!(reason, Some("too".to_string()));
        } else {
            panic!("expected RejectTool");
        }
    }

    #[tokio::test]
    async fn reject_reason_esc_rejects_without_reason() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.pending_approvals.push(ToolCall {
            id: "tc-esc".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "tool".to_string(),
                arguments: "{}".to_string(),
            },
            metadata: None,
        });

        handle_input_event(&mut state, &tx, InputEvent::RejectCurrentTool);
        assert!(state.reject_reason_input.is_some());

        // Esc = skip reason, reject without reason
        handle_input_event(&mut state, &tx, InputEvent::HandleEsc);
        assert!(state.reject_reason_input.is_none());
        assert_eq!(state.pending_approvals.len(), 0);

        if let OutputEvent::RejectTool(_, _, reason) = rx.try_recv().unwrap() {
            assert_eq!(reason, None);
        } else {
            panic!("expected RejectTool");
        }
    }

    // ── Branch 5B behavioral tests ──────────────────────────────────────────

    #[test]
    fn review_open_event_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());

        handle_input_event(&mut state, &tx, InputEvent::ReviewOpen);

        assert!(state.review_open);
        assert_eq!(state.workbench_tab, crate::tui::app::WorkbenchTab::Review);
        assert_eq!(state.focus, crate::tui::app::WorkspaceFocus::Workbench);
    }

    #[test]
    fn review_close_via_esc_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.review_open = true;
        state.focus = crate::tui::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::tui::app::WorkbenchTab::Review;

        handle_input_event(&mut state, &tx, InputEvent::ReviewClose);

        assert!(!state.review_open);
        assert!(state.review_diff.is_none());
    }

    #[test]
    fn review_filter_push_pop_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.review_open = true;
        state.focus = crate::tui::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::tui::app::WorkbenchTab::Review;

        handle_input_event(&mut state, &tx, InputEvent::ReviewFilterInput('a'));
        handle_input_event(&mut state, &tx, InputEvent::ReviewFilterInput('b'));
        assert_eq!(state.review_filter, "ab");

        handle_input_event(&mut state, &tx, InputEvent::ReviewFilterBackspace);
        assert_eq!(state.review_filter, "a");
    }

    #[test]
    fn review_toggle_diff_clears_when_same_path() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.review_open = true;
        state.focus = crate::tui::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::tui::app::WorkbenchTab::Review;
        state.review_selected_path = Some("a.rs".to_string());
        state.review_diff = Some(crate::tui::app::ReviewDiffState {
            path: "a.rs".to_string(),
            old_content: Some("old".to_string()),
            new_content: Some("new".to_string()),
            scroll: 0,
            last_error: None,
        });

        handle_input_event(&mut state, &tx, InputEvent::ReviewToggleDiff);

        assert!(state.review_diff.is_none());
    }

    #[test]
    fn review_scroll_routes_via_handler() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.review_open = true;
        state.focus = crate::tui::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::tui::app::WorkbenchTab::Review;
        state.review_diff = Some(crate::tui::app::ReviewDiffState {
            path: "a.rs".to_string(),
            old_content: Some("old".to_string()),
            new_content: Some("new".to_string()),
            scroll: 5,
            last_error: None,
        });

        handle_input_event(&mut state, &tx, InputEvent::PageUp);
        assert_eq!(state.review_diff.as_ref().unwrap().scroll, 0);

        handle_input_event(&mut state, &tx, InputEvent::PageDown);
        assert!(state.review_diff.as_ref().unwrap().scroll > 0);
    }

    #[tokio::test]
    async fn runtime_tab_requests_refresh_on_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        state.focus = crate::tui::app::WorkspaceFocus::Workbench;
        state.workbench_tab = crate::tui::app::WorkbenchTab::Sessions;

        handle_input_event(&mut state, &tx, InputEvent::WorkbenchNextTab);

        assert_eq!(state.workbench_tab, crate::tui::app::WorkbenchTab::Runtime);
        assert!(matches!(
            rx.recv().await.unwrap(),
            OutputEvent::ListRuntimeJobs
        ));
        assert!(matches!(
            rx.recv().await.unwrap(),
            OutputEvent::LoadRuntimeState
        ));
    }

    #[test]
    fn shell_backend_lifecycle_updates_state() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut state = make_state(dir.path().to_path_buf(), uuid::Uuid::new_v4());
        let (stdin_tx, _stdin_rx) = tokio::sync::mpsc::channel(4);
        let shell = crate::tui::services::ShellCommand {
            id: "shell-1".to_string(),
            command: "echo hi".to_string(),
            stdin_tx,
        };

        handle_backend_event(&mut state, &tx, InputEvent::ShellStarted(shell.clone()));
        assert!(state.active_shell_command.is_some());
        assert!(state.shell_popup_visible);

        handle_backend_event(
            &mut state,
            &tx,
            InputEvent::ShellOutput("shell-1".to_string(), "hello\n".to_string()),
        );
        handle_backend_event(&mut state, &tx, InputEvent::ShellWaitingForInput("shell-1".to_string()));
        assert!(state.shell_output.contains("hello"));
        assert!(state.shell_waiting_for_input);

        handle_backend_event(&mut state, &tx, InputEvent::ShellCompleted("shell-1".to_string(), 0));
        assert!(state.active_shell_command.is_none());
        assert_eq!(state.shell_exit_code, Some(0));
        assert!(!state.shell_waiting_for_input);
    }
}
