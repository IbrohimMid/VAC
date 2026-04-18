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
fn handle_paste_tray_key(state: &mut AppState, c: char) -> bool {
    use crate::tui::services::clipboard_paste as cp;
    let len = state.pending_pastes.len();
    match c {
        'j' if !state.pending_paste_reorder_mode => {
            state.pending_paste_selected = cp::select_next(state.pending_paste_selected, len);
            true
        }
        'k' if !state.pending_paste_reorder_mode => {
            state.pending_paste_selected = cp::select_prev(state.pending_paste_selected, len);
            true
        }
        'J' if state.pending_paste_reorder_mode => {
            state.pending_paste_selected =
                cp::swap_with_next(&mut state.pending_pastes, state.pending_paste_selected);
            true
        }
        'K' if state.pending_paste_reorder_mode => {
            state.pending_paste_selected =
                cp::swap_with_prev(&mut state.pending_pastes, state.pending_paste_selected);
            true
        }
        'd' | 'x' => {
            // Remove from the ledger AND strip the placeholder from input text.
            let sel = state.pending_paste_selected.min(len.saturating_sub(1));
            if sel < state.pending_pastes.len() {
                let placeholder = state.pending_pastes[sel].placeholder.clone();
                // input is empty by gate, but be robust if that changes
                if !state.input.is_empty() {
                    let stripped = state.input.get_content().replace(&placeholder, "");
                    state.input.clear();
                    state.input.insert_str(&stripped);
                }
                state.pending_paste_selected = cp::remove_at(&mut state.pending_pastes, sel);
                if state.pending_pastes.is_empty() {
                    state.pending_paste_reorder_mode = false;
                    state.pending_paste_selected = 0;
                }
            }
            true
        }
        'r' => {
            state.pending_paste_reorder_mode = !state.pending_paste_reorder_mode;
            true
        }
        _ => false,
    }
}

/// Handle input events from user
pub fn handle_input_event(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
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

    // Ask-User popup intercepts all input when visible
    if state.show_ask_user_popup {
        let kind = state.ask_user_question_kind;
        let has_options = !state.ask_user_options.is_empty();
        let max_visible = 10usize;
        let filtered = crate::tui::services::ask_user::filtered_option_indices(
            &state.ask_user_filter,
            &state.ask_user_options,
        );
        let selected_pos = if filtered.is_empty() {
            0
        } else {
            filtered
                .iter()
                .position(|&i| i == state.ask_user_selected)
                .unwrap_or(0)
        };

        match event {
            InputEvent::HandleEsc => {
                // Cancel: send an error-status tool result and close popup.
                if let Some(tc_id) = state.ask_user_tool_call_id.take() {
                    let tool_call = crate::tui::types::ToolCall {
                        id: tc_id.clone(),
                        r#type: "function".to_string(),
                        function: crate::tui::types::FunctionCall {
                            name: crate::tui::services::ask_user::ASK_USER_TOOL_NAME.to_string(),
                            arguments: String::new(),
                        },
                        metadata: None,
                    };
                    let result = crate::tui::types::ToolCallResult {
                        call: tool_call,
                        result: "User cancelled.".to_string(),
                        status: crate::tui::types::ToolCallResultStatus::Error,
                    };
                    let _ =
                        output_tx.try_send(OutputEvent::SendToolResult(result, false, Vec::new()));
                }
                state.show_ask_user_popup = false;
                state.ask_user_question = None;
                state.ask_user_options.clear();
                state.ask_user_input.clear();
                state.ask_user_multi_selected.clear();
                state.ask_user_metadata.clear();
                state.ask_user_filter.clear();
                state.ask_user_search_active = false;
                state.ask_user_scroll = 0;
                return;
            }
            InputEvent::Tab => {
                if has_options {
                    state.ask_user_search_active = !state.ask_user_search_active;
                }
                return;
            }
            InputEvent::Up | InputEvent::ScrollUp => {
                if !filtered.is_empty() && selected_pos > 0 {
                    let new_pos = selected_pos - 1;
                    state.ask_user_selected = filtered[new_pos];
                    state.ask_user_scroll = state.ask_user_scroll.min(new_pos);
                }
                return;
            }
            InputEvent::Down | InputEvent::ScrollDown => {
                if !filtered.is_empty() && selected_pos + 1 < filtered.len() {
                    let new_pos = selected_pos + 1;
                    state.ask_user_selected = filtered[new_pos];
                    let max_scroll = filtered.len().saturating_sub(1);
                    state.ask_user_scroll = state.ask_user_scroll.min(max_scroll);
                    if new_pos >= state.ask_user_scroll.saturating_add(max_visible) {
                        state.ask_user_scroll = new_pos + 1 - max_visible;
                    }
                }
                return;
            }
            InputEvent::InputChanged(c) => {
                if state.ask_user_search_active {
                    state.ask_user_filter.push(c);
                    state.ask_user_scroll = 0;
                    let filtered = crate::tui::services::ask_user::filtered_option_indices(
                        &state.ask_user_filter,
                        &state.ask_user_options,
                    );
                    if let Some(&first) = filtered.first() {
                        state.ask_user_selected = first;
                    }
                    return;
                }

                if kind == crate::tui::services::ask_user::AskUserQuestionKind::MultiSelect
                    && c == ' '
                    && !filtered.is_empty()
                {
                    if state
                        .ask_user_multi_selected
                        .contains(&state.ask_user_selected)
                    {
                        state
                            .ask_user_multi_selected
                            .remove(&state.ask_user_selected);
                    } else {
                        state
                            .ask_user_multi_selected
                            .insert(state.ask_user_selected);
                    }
                    return;
                }

                // Number shortcut: 1-9 selects options[n-1] when free-text is empty.
                if state.ask_user_input.is_empty() && c.is_ascii_digit() && c != '0' {
                    let idx = (c as u8 - b'1') as usize;
                    if idx < filtered.len() {
                        state.ask_user_selected = filtered[idx];
                        return;
                    }
                }
                // Honor `allow_free_text`: when the caller disabled it,
                // typed characters that aren't number shortcuts are dropped.
                if !state.ask_user_allow_free_text {
                    return;
                }
                state.ask_user_input.push(c);
                return;
            }
            InputEvent::InputBackspace => {
                if state.ask_user_search_active {
                    state.ask_user_filter.pop();
                    state.ask_user_scroll = 0;
                    let filtered = crate::tui::services::ask_user::filtered_option_indices(
                        &state.ask_user_filter,
                        &state.ask_user_options,
                    );
                    if let Some(&first) = filtered.first() {
                        state.ask_user_selected = first;
                    }
                    return;
                }
                if state.ask_user_allow_free_text {
                    state.ask_user_input.pop();
                }
                return;
            }
            InputEvent::InputClear => {
                if state.ask_user_search_active {
                    state.ask_user_filter.clear();
                    state.ask_user_scroll = 0;
                    return;
                }
                if kind == crate::tui::services::ask_user::AskUserQuestionKind::MultiSelect {
                    state.ask_user_multi_selected.clear();
                } else if state.ask_user_allow_free_text {
                    state.ask_user_input.clear();
                }
                return;
            }
            InputEvent::InputCursorStart => {
                if kind == crate::tui::services::ask_user::AskUserQuestionKind::MultiSelect {
                    let indices = if state.ask_user_filter.trim().is_empty() {
                        (0..state.ask_user_options.len()).collect::<Vec<_>>()
                    } else {
                        filtered.clone()
                    };
                    state.ask_user_multi_selected = indices.into_iter().collect();
                }
                return;
            }
            InputEvent::InputSubmitted => {
                if let Some(tc_id) = state.ask_user_tool_call_id.take() {
                    let tool_call = crate::tui::types::ToolCall {
                        id: tc_id.clone(),
                        r#type: "function".to_string(),
                        function: crate::tui::types::FunctionCall {
                            name: crate::tui::services::ask_user::ASK_USER_TOOL_NAME.to_string(),
                            arguments: String::new(),
                        },
                        metadata: None,
                    };
                    let payload = crate::tui::services::ask_user::build_answer(
                        state,
                        &state.ask_user_multi_selected,
                    );
                    let answer =
                        serde_json::to_string(&payload).unwrap_or_else(|_| payload.to_string());
                    let result = crate::tui::types::ToolCallResult {
                        call: tool_call,
                        result: answer.clone(),
                        status: crate::tui::types::ToolCallResultStatus::Success,
                    };
                    let _ =
                        output_tx.try_send(OutputEvent::SendToolResult(result, false, Vec::new()));
                    let question = state.ask_user_question.clone().unwrap_or_default();
                    let summary = crate::tui::services::ask_user::answer_summary(state, &filtered);
                    state.push_activity(
                        crate::tui::app::ActivityKind::Approval,
                        crate::tui::services::ask_user::transcript_annotation(&question, &summary),
                    );
                }
                state.show_ask_user_popup = false;
                state.ask_user_question = None;
                state.ask_user_options.clear();
                state.ask_user_input.clear();
                state.ask_user_multi_selected.clear();
                state.ask_user_metadata.clear();
                state.ask_user_filter.clear();
                state.ask_user_search_active = false;
                state.ask_user_scroll = 0;
                return;
            }
            _ => {}
        }
    }

    // Plan review overlay intercepts all input when visible — delegate to domain handler.
    if crate::tui::handlers::plan::handle_plan_review_key(state, &event) {
        return;
    }

    // File changes popup intercepts most input when visible
    if state.show_file_changes_popup {
        let filtered = crate::tui::services::file_changes_popup::filtered_paths(state);
        match event {
            InputEvent::HandleEsc => {
                state.show_file_changes_popup = false;
                return;
            }
            InputEvent::Up | InputEvent::ScrollUp => {
                state.file_changes_selected = state.file_changes_selected.saturating_sub(1);
                if state.file_changes_selected < state.file_changes_scroll {
                    state.file_changes_scroll = state.file_changes_selected;
                }
                return;
            }
            InputEvent::Down | InputEvent::ScrollDown => {
                if !filtered.is_empty() {
                    let max_idx = filtered.len().saturating_sub(1);
                    if state.file_changes_selected < max_idx {
                        state.file_changes_selected += 1;
                    }
                }
                return;
            }
            InputEvent::InputChanged(c) => {
                state.file_changes_search.push(c);
                state.file_changes_selected = 0;
                state.file_changes_scroll = 0;
                return;
            }
            InputEvent::InputBackspace => {
                state.file_changes_search.pop();
                state.file_changes_selected = 0;
                state.file_changes_scroll = 0;
                return;
            }
            InputEvent::ReviewRevertSelected => {
                if let Some(path) = filtered.get(state.file_changes_selected).cloned() {
                    let prior = state.review.selected_path.clone();
                    state.review.selected_path = Some(path);
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = review_handler::revert_selected(&mut ctx);
                    state.review.selected_path = prior;
                }
                return;
            }
            InputEvent::InputSubmitted => {
                if let Some(path) = filtered.get(state.file_changes_selected).cloned() {
                    state.review.selected_path = Some(path);
                    state.workbench_tab = crate::tui::app::WorkbenchTab::Review;
                    state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                    state.show_file_changes_popup = false;
                }
                return;
            }
            _ => {}
        }
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
                    dispatch_builtin_command(state, output_tx, &cmd.command, None);
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
                    crate::tui::app::ShortcutsPopupMode::Commands => {
                        crate::tui::app::ShortcutsPopupMode::Shortcuts
                    }
                    crate::tui::app::ShortcutsPopupMode::Shortcuts => {
                        crate::tui::app::ShortcutsPopupMode::Sessions
                    }
                    crate::tui::app::ShortcutsPopupMode::Sessions => {
                        crate::tui::app::ShortcutsPopupMode::Commands
                    }
                };
                state.shortcuts_scroll = 0;
            }
            InputEvent::Up => {
                state.shortcuts_scroll = state.shortcuts_scroll.saturating_sub(1);
            }
            InputEvent::Down => {
                let max = match state.shortcuts_mode {
                    crate::tui::app::ShortcutsPopupMode::Commands => {
                        crate::tui::services::shortcuts_popup::filter_commands("", state).len()
                    }
                    crate::tui::app::ShortcutsPopupMode::Shortcuts => {
                        crate::tui::services::shortcuts_popup::get_shortcuts_count()
                    }
                    crate::tui::app::ShortcutsPopupMode::Sessions => state.sessions.len(),
                }
                .saturating_sub(1);
                if state.shortcuts_scroll < max {
                    state.shortcuts_scroll += 1;
                }
            }
            InputEvent::InputSubmitted => {
                if state.shortcuts_mode == crate::tui::app::ShortcutsPopupMode::Sessions {
                    if let Some(sel) = state.sessions.get(state.shortcuts_scroll).cloned() {
                        let _ = output_tx.try_send(OutputEvent::SwitchToSession(sel.id));
                        state.push_activity(
                            crate::tui::app::ActivityKind::Session,
                            "Switch session",
                        );
                        state.show_shortcuts = false;
                    }
                } else if state.shortcuts_mode == crate::tui::app::ShortcutsPopupMode::Commands {
                    let cmds = crate::tui::services::shortcuts_popup::filter_commands("", state);
                    if let Some(cmd) = cmds.get(state.shortcuts_scroll) {
                        if let crate::tui::services::shortcuts_popup::CommandAction::InsertSlashCommand(s) = &cmd.action {
                                state.input.clear();
                                state.input.insert_str(s);
                                state.input.input(' ');
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

    if state.review.open
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
            state.message_action_target_id = state
                .messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .map(|m| m.id);
        }
        InputEvent::ToggleSidePanel => {
            state.side_panel_visible = !state.side_panel_visible;
            if !state.side_panel_visible {
                state.side_panel_row_areas.clear();
            }
        }
        InputEvent::MouseDragStart(col, row) => {
            // Banner click regions only react while the banner is still active.
            // Without this guard a click in the stale rect (between TTL expiry
            // and next render) would dismiss or dispatch phantom actions.
            let banner_active = state
                .banner_message
                .as_ref()
                .is_some_and(|m| !m.is_expired());
            if banner_active {
                if let Some(rect) = state.banner_dismiss_region {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        state.banner_message = None;
                        state.banner_click_regions.clear();
                        state.banner_dismiss_region = None;
                        return;
                    }
                }
                let mut banner_action: Option<String> = None;
                for (action, rect) in &state.banner_click_regions {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        banner_action = Some(action.clone());
                        break;
                    }
                }
                if let Some(action) = banner_action {
                    state.banner_message = None;
                    state.banner_click_regions.clear();
                    state.banner_dismiss_region = None;
                    let _ = output_tx.try_send(OutputEvent::UserMessage(
                        action,
                        None,
                        Vec::new(),
                        None,
                    ));
                    return;
                }
            } else if state.banner_message.is_some() {
                // TTL elapsed but not yet re-rendered — clear stale regions proactively.
                state.banner_message = None;
                state.banner_click_regions.clear();
                state.banner_dismiss_region = None;
            }

            let mut clicked_section = None;
            if state.side_panel_visible {
                for (sec, rect) in &state.side_panel_header_areas {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
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
                        if col >= rect.x
                            && col < rect.x + rect.width
                            && row >= rect.y
                            && row < rect.y + rect.height
                        {
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
                                    state.review.selected_path = Some(path);
                                    state.review.selected_idx = 0;
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
            if state.workbench_tab == crate::tui::app::WorkbenchTab::Agents {
                let _ = output_tx.try_send(OutputEvent::ListAgentTasks);
                let _ = output_tx.try_send(OutputEvent::LoadAgentState);
            } else if state.workbench_tab == crate::tui::app::WorkbenchTab::Runtime {
                let _ = output_tx.try_send(OutputEvent::ListRuntimeJobs);
                let _ = output_tx.try_send(OutputEvent::LoadRuntimeState);
            }
        }
        InputEvent::InputChanged(c) => {
            // Unit 5 (Wave 3.1): paste-tray keybindings are live when the
            // input is empty and there are pending pastes. Typing anything
            // into the input re-routes the key to normal handling.
            if state.focus == crate::tui::app::WorkspaceFocus::Input
                && state.input.is_empty()
                && !state.pending_pastes.is_empty()
                && handle_paste_tray_key(state, c)
            {
                return;
            }
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
                    crate::tui::app::WorkbenchTab::Vil => {
                        use crate::tui::handlers::vil_workbench;
                        let mut ctx = HandlerContext::new(state, output_tx);
                        match c {
                            'r' | 'R' => {
                                let _ = vil_workbench::run_repair(&mut ctx);
                            }
                            'a' | 'A' => {
                                let _ = vil_workbench::run_audit(&mut ctx);
                            }
                            'd' | 'D' => {
                                let _ = vil_workbench::run_ir_diff(&mut ctx);
                            }
                            'o' | 'O' => {
                                let _ = vil_workbench::open_in_editor(&mut ctx);
                            }
                            'b' | 'B' => {
                                let _ = vil_workbench::run_batch_campaign(&mut ctx);
                            }
                            _ => {}
                        }
                    }
                    crate::tui::app::WorkbenchTab::Plan => match c {
                        'a' => {
                            plan_write_status(
                                state,
                                crate::tui::services::plan::PlanStatus::Approved,
                            );
                            state.add_assistant_message("Plan approved.".to_string());
                        }
                        'r' => {
                            plan_write_status(
                                state,
                                crate::tui::services::plan::PlanStatus::Drafting,
                            );
                            state.add_assistant_message("Plan marked for revision.".to_string());
                        }
                        'e' => {
                            plan_open_editor(state);
                        }
                        _ => {}
                    },
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
                    crate::tui::app::WorkbenchTab::Agents => {
                        if c == 'r' {
                            let _ = output_tx.try_send(OutputEvent::ListAgentTasks);
                            let _ = output_tx.try_send(OutputEvent::LoadAgentState);
                        }
                    }
                    crate::tui::app::WorkbenchTab::Runtime => match c {
                        'r' => {
                            let _ = output_tx.try_send(OutputEvent::ListRuntimeJobs);
                            let _ = output_tx.try_send(OutputEvent::LoadRuntimeState);
                        }
                        'c' => {
                            if let Some(job) = state.runtime.jobs.get(state.runtime.selected_idx) {
                                let _ = output_tx.try_send(OutputEvent::CancelRuntimeJob(job.id));
                            }
                        }
                        't' => {
                            if let Some(job) = state.runtime.jobs.get(state.runtime.selected_idx) {
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
                let had_pastes = !state.pending_pastes.is_empty();
                let had_images = !state.pending_image_parts.is_empty();
                state.input.clear();
                state.pending_pastes.clear();
                state.pending_image_parts.clear();
                if had_pastes || had_images {
                    state.toasts.push(crate::tui::services::Toast::info(
                        "Input and pending attachments cleared.".to_string(),
                    ));
                }
            }
        }
        InputEvent::InputSubmitted => {
            if state.focus == crate::tui::app::WorkspaceFocus::Input
                && state.shell.popup_visible
                && state.shell.active_command.is_some()
            {
                let text = state.input.get_content();
                let payload = if text.is_empty() {
                    "\n".to_string()
                } else {
                    if state.shell.history.last() != Some(&text) {
                        state.shell.history.push(text.clone());
                    }
                    state.shell.history_idx = None;
                    format!("{text}\n")
                };
                if let Some(shell) = state.shell.active_command.clone() {
                    shell.send_input(payload);
                    state.input.clear();
                    state.shell.waiting_for_input = false;
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

                    if !dispatch_builtin_command(state, output_tx, cmd_word, cmd_args) {
                        let expanded = state.expand_pending_pastes(&msg);
                        state.add_user_message(expanded.clone());
                        let parts = std::mem::take(&mut state.pending_image_parts);
                        let _ = output_tx
                            .try_send(OutputEvent::UserMessage(expanded, None, parts, None));
                    }
                } else {
                    let expanded = state.expand_pending_pastes(&msg);
                    state.add_user_message(expanded.clone());
                    let parts = std::mem::take(&mut state.pending_image_parts);
                    let _ =
                        output_tx.try_send(OutputEvent::UserMessage(expanded, None, parts, None));
                }
            }
        }
        InputEvent::HandlePaste(text) => {
            use crate::tui::services::clipboard_paste::{
                PastedItem, PastedKind, extract_file_paths_from_text, is_long_paste, make_paste_id,
                text_placeholder,
            };
            // First try to extract file paths
            let paths = extract_file_paths_from_text(&text);
            if !paths.is_empty() {
                for path in paths {
                    state.input.insert_str(&path.to_string_lossy());
                    state.input.input(' ');
                }
            } else if is_long_paste(&text) {
                // Long paste → collapse into a placeholder token; full content
                // is expanded at submit time via `expand_pending_pastes`.
                state.paste_counter += 1;
                let id = make_paste_id(state.paste_counter);
                let char_count = text.chars().count();
                let line_count = text.chars().filter(|c| *c == '\n').count() + 1;
                let placeholder = text_placeholder(&id, char_count, line_count);
                state.input.insert_str(&placeholder);
                state.input.input(' ');
                state.pending_pastes.push(PastedItem {
                    id,
                    placeholder,
                    kind: PastedKind::Text {
                        content: text,
                        line_count,
                        char_count,
                    },
                });
            } else {
                // Short paste → inline directly.
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
                    Ok((path, info)) => {
                        if let Ok(bytes) = std::fs::read(&path) {
                            if bytes.len() > MAX_IMAGE_BYTES {
                                log::warn!(
                                    "Image too large ({} bytes), max {} bytes",
                                    bytes.len(),
                                    MAX_IMAGE_BYTES
                                );
                                state.input.insert_str("[image too large, max 10MB] ");
                            } else {
                                use crate::tui::services::clipboard_paste::{
                                    PastedItem, PastedKind, image_placeholder, make_paste_id,
                                };
                                use base64::Engine as _;
                                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                                let media_type = match path.extension().and_then(|e| e.to_str()) {
                                    Some("png") => "image/png",
                                    Some("jpg") | Some("jpeg") => "image/jpeg",
                                    Some("gif") => "image/gif",
                                    Some("webp") => "image/webp",
                                    _ => "image/png",
                                };
                                let part = crate::tui::types::ContentPart {
                                    r#type: "image".to_string(),
                                    text: None,
                                    image_url: Some(crate::tui::types::ImageUrl {
                                        url: format!("data:{};base64,{}", media_type, b64),
                                    }),
                                };
                                state.pending_image_parts.push(part);
                                state.paste_counter += 1;
                                let id = make_paste_id(state.paste_counter);
                                let placeholder = image_placeholder(&id, info.width, info.height);
                                state.input.insert_str(&placeholder);
                                state.input.input(' ');
                                state.pending_pastes.push(PastedItem {
                                    id,
                                    placeholder,
                                    kind: PastedKind::Image {
                                        width: info.width,
                                        height: info.height,
                                        byte_count: bytes.len(),
                                    },
                                });
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
                if state.shell.popup_visible
                    && state.shell.active_command.is_some()
                    && !state.shell.history.is_empty()
                {
                    let max_idx = state.shell.history.len() - 1;
                    let next_idx = match state.shell.history_idx {
                        Some(idx) => idx.saturating_sub(1),
                        None => max_idx,
                    };
                    state.shell.history_idx = Some(next_idx);
                    if let Some(cmd) = state.shell.history.get(next_idx) {
                        state.input.set_content(cmd);
                        state.input.move_cursor_end();
                    }
                } else {
                    state.input.move_cursor_up();
                }
            }
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
                crate::tui::app::WorkbenchTab::Agents => {
                    state.runtime.agent_selected = state.runtime.agent_selected.saturating_sub(1);
                    state.runtime.agent_detail_scroll = 0;
                }
                crate::tui::app::WorkbenchTab::Runtime => {
                    state.runtime.selected_idx = state.runtime.selected_idx.saturating_sub(1);
                    state.runtime.detail_scroll = 0;
                }
                crate::tui::app::WorkbenchTab::Review => {
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = review_handler::select_prev(&mut ctx);
                }
                crate::tui::app::WorkbenchTab::Plan => {}
                crate::tui::app::WorkbenchTab::Vil => {
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = crate::tui::handlers::vil_workbench::select_prev(&mut ctx);
                }
            },
        },
        InputEvent::Down => match state.focus {
            crate::tui::app::WorkspaceFocus::Input => {
                if state.shell.popup_visible
                    && state.shell.active_command.is_some()
                    && state.shell.history_idx.is_some()
                {
                    let next_idx = state.shell.history_idx.unwrap() + 1;
                    if next_idx >= state.shell.history.len() {
                        state.shell.history_idx = None;
                        state.input.clear();
                    } else {
                        state.shell.history_idx = Some(next_idx);
                        if let Some(cmd) = state.shell.history.get(next_idx) {
                            state.input.set_content(cmd);
                            state.input.move_cursor_end();
                        }
                    }
                } else {
                    state.input.move_cursor_down();
                }
            }
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
                crate::tui::app::WorkbenchTab::Agents => {
                    if state.runtime.agent_selected + 1 < state.runtime.agent_tasks.len() {
                        state.runtime.agent_selected += 1;
                        state.runtime.agent_detail_scroll = 0;
                    }
                }
                crate::tui::app::WorkbenchTab::Runtime => {
                    if state.runtime.selected_idx + 1 < state.runtime.jobs.len() {
                        state.runtime.selected_idx += 1;
                        state.runtime.detail_scroll = 0;
                    }
                }
                crate::tui::app::WorkbenchTab::Review => {
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = review_handler::select_next(&mut ctx);
                }
                crate::tui::app::WorkbenchTab::Plan => {}
                crate::tui::app::WorkbenchTab::Vil => {
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = crate::tui::handlers::vil_workbench::select_next(&mut ctx);
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
            } else if state.shell.popup_visible && state.shell.active_command.is_some() {
                state.shell.popup_visible = false;
                state.shell.backgrounded = true;
            } else if state.is_streaming {
                let _ = output_tx.try_send(OutputEvent::CancelStream);
                state.is_streaming = false;
            } else if state.focus == crate::tui::app::WorkspaceFocus::Workbench
                && state.workbench_tab == crate::tui::app::WorkbenchTab::Review
            {
                state.review.open = false;
                state.review.diff = None;
            }
        }
        InputEvent::HandleCtrlZ => {
            if state.shell.active_command.is_some() {
                if state.shell.popup_visible {
                    state.shell.popup_visible = false;
                    state.shell.backgrounded = true;
                } else {
                    state.shell.popup_visible = true;
                    state.shell.backgrounded = false;
                }
                state.shell.history_idx = None;
            }
        }
        InputEvent::BackgroundShell => {
            if state.shell.active_command.is_some() {
                state.shell.popup_visible = false;
                state.shell.backgrounded = true;
            }
        }
        InputEvent::FocusShell => {
            if state.shell.active_command.is_some() {
                state.shell.popup_visible = true;
                state.shell.backgrounded = false;
            }
        }
        InputEvent::ShellKill => {
            if let Some(shell) = state.shell.active_command.clone() {
                let _ = shell.kill();
                state.shell.waiting_for_input = false;
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

    // Fast path: the line→message map is populated by `render_messages` on
    // every frame, so a direct index lookup is O(1) and stays consistent with
    // what the user actually sees.
    if let Some(id) = state.line_to_message_map.get(line_idx).copied() {
        return Some(id);
    }

    // Fallback for the first-click-before-first-render race: walk messages and
    // accumulate line counts from per_message_cache.
    let mut cumulative = 0usize;
    for msg in &state.messages {
        let msg_lines = state
            .per_message_cache
            .get(&msg.id)
            .map(|c| c.rendered_lines.len() + 1)
            .unwrap_or(1);
        if line_idx < cumulative + msg_lines {
            return Some(msg.id);
        }
        cumulative += msg_lines;
    }
    None
}

/// Estimate context-window occupancy as a percent of the active model's
/// window. Model records don't carry a window size today, so we use coarse
/// provider/name heuristics (Claude 4.x = 200k; GPT-4o = 128k; GPT-4 = 8k;
/// unknown = 200k as a safe default). Returns 0 when tokens_used is zero.
fn estimate_context_percent(model: Option<&crate::tui::types::Model>, tokens_used: u64) -> f32 {
    if tokens_used == 0 {
        return 0.0;
    }
    let window: u64 = match model {
        Some(m) => {
            let id = m.id.to_lowercase();
            let name = m.name.to_lowercase();
            if id.contains("claude")
                || name.contains("claude")
                || id.contains("sonnet")
                || id.contains("opus")
                || id.contains("haiku")
            {
                200_000
            } else if id.contains("gpt-4o") || name.contains("gpt-4o") {
                128_000
            } else if id.contains("gpt-4") || name.contains("gpt-4") {
                8_192
            } else if id.contains("gemini") {
                1_000_000
            } else {
                200_000
            }
        }
        None => 200_000,
    };
    ((tokens_used as f64 / window as f64) * 100.0).clamp(0.0, 100.0) as f32
}

/// Suspend the TUI, launch `$EDITOR` on plan.md, then re-read the file on
/// return. Mirrors `handlers::review::open_editor` so editor integration stays
/// consistent across the app.
fn plan_open_editor(state: &mut AppState) {
    crate::tui::handlers::plan::open_editor(state);
}

fn plan_write_status(state: &mut AppState, new_status: crate::tui::services::plan::PlanStatus) {
    crate::tui::handlers::plan::write_plan_status(state, new_status);
}

pub(crate) fn open_ask_user_popup(state: &mut AppState, tc: &crate::tui::types::ToolCall) {
    let args = crate::tui::services::ask_user::parse_args(&tc.function.arguments);
    // Default policy: allow free-text iff the caller opts in OR no options
    // were supplied (otherwise the user would have no way to answer).
    let (question, options, allow_free_text, kind, metadata) = match args {
        Some(a) => {
            let k = a.effective_kind();
            let free = matches!(
                k,
                crate::tui::services::ask_user::AskUserQuestionKind::FreeText
                    | crate::tui::services::ask_user::AskUserQuestionKind::Mixed
            ) || a.allow_free_text
                || a.options.is_empty();
            (Some(a.question), a.options, free, k, a.metadata)
        }
        None => (
            Some("The assistant needs more information.".to_string()),
            Vec::new(),
            true,
            crate::tui::services::ask_user::AskUserQuestionKind::FreeText,
            std::collections::HashMap::new(),
        ),
    };
    state.ask_user_question = question;
    state.ask_user_options = options;
    state.ask_user_selected = 0;
    state.ask_user_input.clear();
    state.ask_user_allow_free_text = allow_free_text;
    state.ask_user_question_kind = kind;
    state.ask_user_metadata = metadata;
    state.ask_user_multi_selected.clear();
    state.ask_user_filter.clear();
    state.ask_user_search_active = false;
    state.ask_user_scroll = 0;
    state.ask_user_tool_call_id = Some(tc.id.clone());
    state.show_ask_user_popup = true;
    state.push_activity(
        crate::tui::app::ActivityKind::Approval,
        "Assistant requested input",
    );
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

fn is_vil_tool(tool_name: &str) -> bool {
    tool_name.starts_with("vil_")
}

fn truncate_banner_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars()
        .take(max_chars.saturating_sub(1))
        .chain(std::iter::once('…'))
        .collect()
}

pub(crate) fn classify_critical_banner(
    text: &str,
) -> Option<(
    crate::tui::services::banner::BannerStyle,
    crate::tui::services::banner::BannerSeverity,
)> {
    let lower = text.to_ascii_lowercase();
    let tls_related = lower.contains("tls")
        || lower.contains("certificate")
        || lower.contains("ca file")
        || lower.contains("mtls")
        || lower.contains("server_name")
        || lower.contains("identity");

    if lower.contains("warden blocked")
        || lower.contains("trust/policy violation")
        || lower.contains("permission denied")
        || lower.contains("blocked:")
        || lower.contains("blocked by")
    {
        return Some((
            crate::tui::services::banner::BannerStyle::Error,
            crate::tui::services::banner::BannerSeverity::Blocking,
        ));
    }

    if lower.contains("failed to connect mcp server") || lower.contains("mcp error") {
        return Some(if tls_related {
            (
                crate::tui::services::banner::BannerStyle::Error,
                crate::tui::services::banner::BannerSeverity::Blocking,
            )
        } else {
            (
                crate::tui::services::banner::BannerStyle::Warning,
                crate::tui::services::banner::BannerSeverity::Suggested,
            )
        });
    }

    if lower.contains("agent loop exceeded")
        || lower.contains("iterations without completing")
        || lower.contains("max iterations")
    {
        return Some((
            crate::tui::services::banner::BannerStyle::Error,
            crate::tui::services::banner::BannerSeverity::Blocking,
        ));
    }

    if lower.contains("rate limited") || lower.contains("retry after") || lower.contains("429") {
        return Some((
            crate::tui::services::banner::BannerStyle::Warning,
            crate::tui::services::banner::BannerSeverity::Suggested,
        ));
    }

    if lower.contains("all providers in fallback chain failed")
        || lower.contains("maximum retry attempts")
        || lower.contains("max retry")
    {
        return Some((
            crate::tui::services::banner::BannerStyle::Error,
            crate::tui::services::banner::BannerSeverity::Suggested,
        ));
    }

    None
}

fn push_banner_direct(
    state: &mut AppState,
    text: String,
    style: crate::tui::services::banner::BannerStyle,
    severity: crate::tui::services::banner::BannerSeverity,
) {
    let msg = crate::tui::services::banner::BannerMessage::new(text, style).with_severity(severity);
    state.banner_queue.push(msg);
    state.banner_message = state.banner_queue.current().cloned();
}

fn policy_gate_allows_shell_command(state: &mut AppState, cmd: &str) -> bool {
    let Ok(config) = vac_core::VacConfig::load_with_fallback(&state.project_root) else {
        return true;
    };
    let Some(action) = vac_core::policy_gate::classify_shell_command(cmd) else {
        return true;
    };
    let decision =
        vac_core::policy_gate::evaluate(&config.policy_gate, action, state.vil.last_score);
    match decision {
        vac_core::policy_gate::PolicyGateDecision::Allow => true,
        vac_core::policy_gate::PolicyGateDecision::Warn(msg) => {
            push_banner_direct(
                state,
                truncate_banner_text(&msg, 140),
                crate::tui::services::banner::BannerStyle::Warning,
                crate::tui::services::banner::BannerSeverity::Suggested,
            );
            true
        }
        vac_core::policy_gate::PolicyGateDecision::Block(msg) => {
            push_banner_direct(
                state,
                truncate_banner_text(&msg, 140),
                crate::tui::services::banner::BannerStyle::Error,
                crate::tui::services::banner::BannerSeverity::Blocking,
            );
            false
        }
    }
}

pub fn handle_backend_event(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    match event {
        InputEvent::AssistantMessage(msg) => {
            let extracted = crate::tui::services::todo_extractor::extract_todos(&msg);
            if !extracted.is_empty() {
                state.todos = extracted;
            }
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
            // Stream/LLM turns end here; scan the most recent assistant message
            // for a `<todo>` block and refresh the side-panel surface.
            if let Some(last) = state.messages.iter().rev().find(|m| m.role == "assistant") {
                let extracted = crate::tui::services::todo_extractor::extract_todos(&last.content);
                if !extracted.is_empty() {
                    state.todos = extracted;
                }
            }
            state.push_activity(
                crate::tui::app::ActivityKind::Status,
                format!("Done: {op_label}"),
            );
        }
        InputEvent::Error(msg) => {
            if let Some((style, severity)) = classify_critical_banner(&msg) {
                push_banner_direct(state, truncate_banner_text(&msg, 140), style, severity);
            }
            let lower = msg.to_ascii_lowercase();
            if lower.contains("vil") || lower.contains("ir") || lower.contains("rulebook") {
                state.push_vil_log(format!("error: {}", truncate_banner_text(&msg, 120)));
            }
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
        InputEvent::ShowBanner(text, style, severity) => {
            let msg = crate::tui::services::banner::BannerMessage::new(text, style)
                .with_severity(severity);
            state.banner_queue.push(msg);
            state.banner_message = state.banner_queue.current().cloned();
        }
        InputEvent::ShowToast(toast) => {
            let lower = toast.message.to_ascii_lowercase();
            if lower.contains("vil")
                || lower.contains("rulebook")
                || lower.contains("archetype")
                || lower.contains("ir")
            {
                state.push_vil_log(toast.message.clone());
            }
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
        InputEvent::SetAgentTasks(tasks) => {
            state.runtime.agent_tasks = tasks;
            if state.runtime.agent_selected >= state.runtime.agent_tasks.len() {
                state.runtime.agent_selected = state.runtime.agent_tasks.len().saturating_sub(1);
            }
            state.push_activity(crate::tui::app::ActivityKind::Status, "Agent queue updated");
        }
        InputEvent::SetAgentState(snapshot) => {
            state.runtime.agent_snapshot = snapshot;
            state.push_activity(crate::tui::app::ActivityKind::Status, "Agent state updated");
        }
        InputEvent::SetRuntimeJobs(jobs) => {
            state.runtime.jobs = jobs;
            if state.runtime.selected_idx >= state.runtime.jobs.len() {
                state.runtime.selected_idx = state.runtime.jobs.len().saturating_sub(1);
            }
            state.push_activity(
                crate::tui::app::ActivityKind::Status,
                "Runtime jobs updated",
            );
        }
        InputEvent::SetRuntimeState(snapshot) => {
            // Detect significant state changes for activity logging
            let prev_state = state.runtime.snapshot.as_ref().map(|s| &s.state);
            let new_state = snapshot.as_ref().map(|s| &s.state);

            if let (Some(prev), Some(new)) = (prev_state, new_state) {
                match new {
                    vac_runtime::AutopilotState::WaitingApproval { tool_call_id } => {
                        if !matches!(prev, vac_runtime::AutopilotState::WaitingApproval { .. }) {
                            state.push_activity(
                                crate::tui::app::ActivityKind::Approval,
                                format!("Runtime waiting for approval: {}", &tool_call_id[..8]),
                            );
                            state.toasts.push(crate::tui::services::Toast::info(format!(
                                "Runtime waiting for approval: {}",
                                &tool_call_id[..8]
                            )));
                        }
                    }
                    vac_runtime::AutopilotState::Backoff { until } => {
                        if !matches!(prev, vac_runtime::AutopilotState::Backoff { .. }) {
                            state.push_activity(
                                crate::tui::app::ActivityKind::Status,
                                format!(
                                    "Runtime entered backoff until {}",
                                    until.format("%H:%M:%S")
                                ),
                            );
                            state.toasts.push(crate::tui::services::Toast::info(format!(
                                "Runtime backoff until {}",
                                until.format("%H:%M:%S")
                            )));
                        }
                    }
                    _ => {}
                }
            }

            // Detect execution environment changes
            if let Some(new_snapshot) = &snapshot {
                if let Some(prev_snapshot) = &state.runtime.snapshot {
                    if prev_snapshot.execution_environment != new_snapshot.execution_environment {
                        let env_name = match new_snapshot.execution_environment {
                            vac_core::ExecutionEnvironment::Host => "host",
                            vac_core::ExecutionEnvironment::IsolatedBatch => "isolated-batch",
                            vac_core::ExecutionEnvironment::IsolatedInteractive => {
                                "isolated-interactive"
                            }
                        };
                        state.push_activity(
                            crate::tui::app::ActivityKind::Status,
                            format!("Execution environment switched to {}", env_name),
                        );
                        state.toasts.push(crate::tui::services::Toast::info(format!(
                            "Switched to {} environment",
                            env_name
                        )));
                    }
                }
            }

            state.runtime.snapshot = snapshot;
        }
        InputEvent::FileIndexReady(files) => {
            state.all_files = files;
            state.file_search_results = state.all_files.iter().take(50).cloned().collect();
            state.toasts.push(crate::tui::services::Toast::success(
                "File index ready".to_string(),
            ));
        }
        InputEvent::ShellStarted(shell) => {
            state.shell.active_command = Some(shell.clone());
            state.shell.popup_visible = true;
            state.shell.backgrounded = false;
            state.shell.waiting_for_input = false;
            state.shell.exit_code = None;
            state.shell.last_error = None;
            state.shell.output.clear();
            state.shell.history_idx = None;
            state.push_activity(
                crate::tui::app::ActivityKind::Shell,
                format!("Shell started: {}", shell.command),
            );
        }
        InputEvent::ShellOutput(id, text) => {
            if state
                .shell.active_command
                .as_ref()
                .is_some_and(|cmd| cmd.id == id)
                || id == "system"
            {
                state.shell.output.push_str(&text);

                // Truncate to 1MB circular buffer
                let max_size = 1024 * 1024;
                if state.shell.output.len() > max_size {
                    let keep_len = max_size - 128 * 1024; // Keep ~900KB to avoid shifting on every push
                    let mut safe_idx = state.shell.output.len() - keep_len;
                    while safe_idx < state.shell.output.len()
                        && !state.shell.output.is_char_boundary(safe_idx)
                    {
                        safe_idx += 1;
                    }
                    state.shell.output = state.shell.output[safe_idx..].to_string();
                }

                // Unit 6: detect prompt-ready and password-mode from output
                state.shell.prompt_ready =
                    crate::tui::services::shell_mode::detect_prompt_ready(&state.shell.output);
                state.shell.password_mode =
                    crate::tui::services::shell_mode::detect_password_prompt(&text);
                if state.shell.prompt_ready {
                    state.shell.lifecycle =
                        crate::tui::services::shell_mode::ShellLifecycle::PromptReady;
                } else {
                    state.shell.lifecycle =
                        crate::tui::services::shell_mode::ShellLifecycle::Running;
                }
            }
        }
        InputEvent::ShellError(id, text) => {
            if state
                .shell.active_command
                .as_ref()
                .is_some_and(|cmd| cmd.id == id)
                || id == "system"
            {
                if !state.shell.output.ends_with('\n') && !state.shell.output.is_empty() {
                    state.shell.output.push('\n');
                }
                state
                    .shell.output
                    .push_str(&format!("[shell error] {text}\n"));

                let max_size = 1024 * 1024;
                if state.shell.output.len() > max_size {
                    let keep_len = max_size - 128 * 1024;
                    let mut safe_idx = state.shell.output.len() - keep_len;
                    while safe_idx < state.shell.output.len()
                        && !state.shell.output.is_char_boundary(safe_idx)
                    {
                        safe_idx += 1;
                    }
                    state.shell.output = state.shell.output[safe_idx..].to_string();
                }

                state.shell.last_error = Some(text.clone());
                state.shell.lifecycle =
                    crate::tui::services::shell_mode::ShellLifecycle::Error(text.clone());
            }
            state.push_activity(
                crate::tui::app::ActivityKind::Shell,
                format!("Shell error: {}", text),
            );
        }
        InputEvent::ShellCompleted(id, code) => {
            if state
                .shell.active_command
                .as_ref()
                .is_some_and(|cmd| cmd.id == id)
                || id == "system"
            {
                state.shell.exit_code = Some(code);
                state.shell.waiting_for_input = false;
                state.shell.active_command = None;
                state.shell.prompt_ready = false;
                state.shell.password_mode = false;
                // Unit 6: distinguish normal exit from killed (code -1 or 137)
                state.shell.lifecycle = if code == -1 || code == 137 {
                    crate::tui::services::shell_mode::ShellLifecycle::Killed
                } else {
                    crate::tui::services::shell_mode::ShellLifecycle::Exited(code)
                };
            }
            let status = if code == 0 { "success" } else { "failed" };
            state.push_activity(
                crate::tui::app::ActivityKind::Shell,
                format!("Shell {} (exit code {})", status, code),
            );
        }
        InputEvent::ShellWaitingForInput(id) => {
            if state
                .shell.active_command
                .as_ref()
                .is_some_and(|cmd| cmd.id == id)
                || id == "system"
            {
                state.shell.waiting_for_input = true;
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
            if matches!(
                conn_state.status,
                vac_tools::mcp::McpConnectionStatus::Unreachable(_)
            ) {
                let reason = match &conn_state.status {
                    vac_tools::mcp::McpConnectionStatus::Unreachable(r) => r.clone(),
                    _ => String::new(),
                };
                push_banner_direct(
                    state,
                    truncate_banner_text(
                        &format!("MCP server '{name}' unreachable: {reason}"),
                        140,
                    ),
                    crate::tui::services::banner::BannerStyle::Warning,
                    crate::tui::services::banner::BannerSeverity::Suggested,
                );
            }
            state.mcp_server_states.insert(name, conn_state);
        }
        InputEvent::VilStatusUpdated(snapshot) => {
            state.record_vil_score(snapshot.validation_score);
            state.push_vil_log(format!(
                "vil_status score={:.2} issues={}",
                snapshot.validation_score,
                snapshot.validation_issues.len()
            ));
            state.vil.status = snapshot;
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
                            let profile =
                                vac_core::detector::VilProjectProfile::detect(&project_root);
                            let mut ir_metadata_files = vec![];
                            for (path, module) in pipeline.modules() {
                                let has_vil_attr =
                                    module.structs.iter().any(|s| !s.vil_attrs.is_empty())
                                        || module.functions.iter().any(|f| !f.vil_attrs.is_empty());
                                if has_vil_attr {
                                    ir_metadata_files.push(path.clone());
                                }
                            }

                            let config = vac_core::VacConfig::load_with_fallback(&project_root)
                                .unwrap_or_default();
                            let semantic_mode = config.memory.enable_semantic;

                            let active_rulebook = {
                                let books = vac_core::rulebook::RulebookLoader::load_all(
                                    &project_root,
                                    &config.rulebook.paths,
                                );
                                if books.is_empty() {
                                    None
                                } else {
                                    Some(
                                        books
                                            .iter()
                                            .map(|b| b.id.clone())
                                            .collect::<Vec<_>>()
                                            .join(", "),
                                    )
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
        InputEvent::IsolationBoundary {
            action,
            environment,
        } => {
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
            state.review.open = false;
            state.review.filter.clear();
            state.review.diff = None;
            state.review.selected_idx = 0;
            state.review.selected_path = None;
            state.shell.active_command = None;
            state.shell.popup_visible = false;
            state.shell.output.clear();
            state.shell.waiting_for_input = false;
            state.shell.backgrounded = false;
            state.shell.exit_code = None;
            state.shell.last_error = None;
            state.shell.history_idx = None;
            state.runtime.jobs.clear();
            state.runtime.selected_idx = 0;
            state.runtime.filter.clear();
            state.runtime.detail_scroll = 0;
            state.runtime.snapshot = None;
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
            state.vil.score_history.clear();
            state.vil.event_log.clear();
            state.vil.last_score = None;
            state.workbench_tab = crate::tui::app::WorkbenchTab::Approvals;
            state.focus = crate::tui::app::WorkspaceFocus::Input;
            state.push_activity(crate::tui::app::ActivityKind::Session, "Session restored");
        }
        InputEvent::ShowConfirmationDialog(tc) => {
            if tc.function.name == crate::tui::services::ask_user::ASK_USER_TOOL_NAME {
                open_ask_user_popup(state, &tc);
                return;
            }
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
            if tc.function.name == crate::tui::services::ask_user::ASK_USER_TOOL_NAME {
                open_ask_user_popup(state, &tc);
                return;
            }
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
            if is_vil_tool(&tc.function.name) {
                state.push_vil_log(format!("tool start {}", tc.function.name));
            }
        }
        InputEvent::ToolResult(result) => {
            state.pending_tool_calls.retain(|c| c.id != result.call.id);
            state.approved_tools.retain(|c| c.id != result.call.id);
            if result.status == crate::tui::types::ToolCallResultStatus::Error {
                if let Some((style, severity)) = classify_critical_banner(&result.result) {
                    push_banner_direct(
                        state,
                        truncate_banner_text(&result.result, 140),
                        style,
                        severity,
                    );
                }
            }
            state.add_assistant_message(result.result);
            state.push_activity(
                crate::tui::app::ActivityKind::Tool,
                format!("Tool result: {}", result.call.function.name),
            );
            if is_vil_tool(&result.call.function.name) {
                let status = match result.status {
                    crate::tui::types::ToolCallResultStatus::Success => "ok",
                    crate::tui::types::ToolCallResultStatus::Error => "error",
                    crate::tui::types::ToolCallResultStatus::Pending => "pending",
                };
                state.push_vil_log(format!("tool {status} {}", result.call.function.name));
            }
        }
        InputEvent::TaskCompleted(result) => {
            state.vil.last_score = result.validation_score;
            // Real usage wiring: `vac_core::task::TaskResult.total_tokens_used`
            // is the authoritative producer. We record this turn's total, add
            // to session running total, and derive a coarse context %.
            let turn_tokens = result.total_tokens_used;
            state.current_message_usage = crate::tui::app::TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: turn_tokens,
            };
            state.total_session_usage.total_tokens = state
                .total_session_usage
                .total_tokens
                .saturating_add(turn_tokens);
            state.context_usage_percent =
                estimate_context_percent(state.current_model.as_ref(), turn_tokens);

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

            if state.review.open {
                state.review.generation = state.review.generation.saturating_add(1);
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



pub fn dispatch_builtin_command(
    state: &mut AppState,
    output_tx: &tokio::sync::mpsc::Sender<OutputEvent>,
    cmd_word: &str,
    cmd_args: Option<&str>,
) -> bool {
    let trimmed = if let Some(args) = cmd_args {
        format!("{} {}", cmd_word, args)
    } else {
        cmd_word.to_string()
    };
    if let Some(cmd) = state.commands.iter().find(|c| c.command == cmd_word).cloned() {
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
                } else if cmd.command == "/agents" {
                    state.workbench_tab = crate::tui::app::WorkbenchTab::Agents;
                    state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                    let _ = output_tx.try_send(OutputEvent::ListAgentTasks);
                    let _ = output_tx.try_send(OutputEvent::LoadAgentState);
                } else if cmd.command == "/shell" {
                    state.add_user_message(trimmed.clone());
                    let shell_cmd = cmd_args.unwrap_or_default().to_string();
                    if policy_gate_allows_shell_command(state, &shell_cmd) {
                        let _ = output_tx.try_send(OutputEvent::ExecuteCommand(
                            shell_cmd,
                            state.active_isolation_mode.clone(),
                        ));
                    }
                } else if cmd.command == "/shell-focus" {
                    if state.shell.active_command.is_some() {
                        state.shell.popup_visible = true;
                        state.shell.backgrounded = false;
                    }
                } else if cmd.command == "/shell-bg" {
                    if state.shell.active_command.is_some() {
                        state.shell.popup_visible = false;
                        state.shell.backgrounded = true;
                    }
                } else if cmd.command == "/shell-kill" {
                    if let Some(shell) = state.shell.active_command.clone() {
                        let _ = shell.kill();
                    }
                } else if cmd.command == "/new" {
                    let _ = output_tx.try_send(OutputEvent::NewSession);
                } else if cmd.command == "/export" {
                    state.add_user_message(trimmed.clone());
                    let output_path = cmd_args
                        .map(std::path::PathBuf::from)
                        .map(|p| {
                            if p.is_absolute() {
                                p
                            } else {
                                state.project_root.join(p)
                            }
                        })
                        .unwrap_or_else(|| {
                            state
                                .project_root
                                .join(".vac/exports/session.bundle.json")
                        });
                    state.toasts.push(crate::tui::services::Toast::info(
                        "Mengekspor bundle...".to_string(),
                    ));
                    let _ =
                        output_tx.try_send(OutputEvent::ExportBundle(output_path));
                } else if cmd.command == "/import" {
                    state.add_user_message(trimmed.clone());
                    let Some(arg) = cmd_args else {
                        state.toasts.push(crate::tui::services::Toast::error(
                            "Gunakan: /import <path>".to_string(),
                        ));
                        state.add_assistant_message(
                            "Gunakan: /import <path>".to_string(),
                        );
                        return true;
                    };
                    let mut input_path = std::path::PathBuf::from(arg);
                    if !input_path.is_absolute() {
                        input_path = state.project_root.join(input_path);
                    }
                    state.toasts.push(crate::tui::services::Toast::info(
                        "Mengimpor bundle...".to_string(),
                    ));
                    let _ =
                        output_tx.try_send(OutputEvent::ImportBundle(input_path));
                } else if cmd.command == "/review" {
                    state.add_user_message(trimmed.clone());
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = review_handler::open(&mut ctx);
                } else if cmd.command == "/model" {
                    state.add_user_message(trimmed.clone());
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = model_switcher::open(&mut ctx);
                } else if cmd.command == "/files" {
                    state.add_user_message(trimmed.clone());
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = file_search::open(&mut ctx);
                } else if cmd.command == "/changes" {
                    state.add_user_message(trimmed.clone());
                    let mut ctx = HandlerContext::new(state, output_tx);
                    let _ = changeset_handler::open(&mut ctx);
                } else if cmd.command == "/file-changes" {
                    state.show_file_changes_popup = true;
                    state.file_changes_selected = 0;
                    state.file_changes_scroll = 0;
                    state.file_changes_search.clear();
                } else if cmd.command == "/plan" {
                    state.add_user_message(trimmed.clone());
                    let project_root = state.project_root.clone();
                    if let Some((meta, content)) =
                        crate::tui::services::plan::read_plan_file(&project_root)
                    {
                        state.plan.metadata = Some(meta);
                        state.plan.draft = content;
                    } else {
                        let title = state
                            .session_title
                            .clone()
                            .unwrap_or_else(|| "Session Plan".to_string());
                        let tmpl =
                            crate::tui::services::plan::new_plan_template(&title);
                        if let Err(e) = crate::tui::services::plan::write_plan_file(
                            &project_root,
                            &tmpl,
                        ) {
                            state.add_assistant_message(format!(
                                "Failed to create plan: {}",
                                e
                            ));
                        } else {
                            state.plan.metadata =
                                crate::tui::services::plan::parse_plan_front_matter(
                                    &tmpl,
                                );
                            state.plan.draft = tmpl;
                        }
                    }
                    state.plan.mode_active = true;
                    state.workbench_tab = crate::tui::app::WorkbenchTab::Plan;
                    state.focus = crate::tui::app::WorkspaceFocus::Workbench;
                } else if cmd.command == "/plan-review" {
                    let project_root = state.project_root.clone();
                    if let Some((meta, content)) =
                        crate::tui::services::plan::read_plan_file(&project_root)
                    {
                        state.plan.metadata = Some(meta);
                        state.plan.draft = content;
                        state.plan.review_open = true;
                        state.plan.review_selected = 0;
                        state.plan.review_scroll = 0;
                    } else {
                        state.add_assistant_message(
                            "No plan.md yet. Run /plan first.".to_string(),
                        );
                    }
                } else if cmd.command == "/plan-edit" {
                    state.add_user_message(trimmed.clone());
                    plan_open_editor(state);
                } else {
                    let expanded = state.expand_pending_pastes(&trimmed);
                    state.add_user_message(expanded.clone());
                    let parts = std::mem::take(&mut state.pending_image_parts);
                    let _ = output_tx.try_send(OutputEvent::UserMessage(
                        expanded, None, parts, None,
                    ));
                }
            }
            crate::tui::app::CommandSource::BuiltInWithPrompt {
                prompt_content,
            }
            | crate::tui::app::CommandSource::Custom { prompt_content } => {
                state.add_user_message(trimmed.clone());
                let prompt = match cmd_args {
                    Some(args) => format!("{}\n\n{}", prompt_content, args),
                    None => prompt_content,
                };
                let _ = output_tx.try_send(OutputEvent::UserMessage(
                    prompt,
                    None,
                    vec![],
                    None,
                ));
            }
            // Passthrough: no TUI handler; forward verbatim to the agent.
            crate::tui::app::CommandSource::Passthrough => {
                let expanded = state.expand_pending_pastes(&trimmed);
                state.add_user_message(expanded.clone());
                let parts = std::mem::take(&mut state.pending_image_parts);
                let _ = output_tx.try_send(OutputEvent::UserMessage(
                    expanded, None, parts, None,
                ));
            }
        }
        return true;
    }
    false
}
