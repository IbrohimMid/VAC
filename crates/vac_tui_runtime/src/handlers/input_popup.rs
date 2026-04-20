//! Popup/overlay input dispatcher.
//!
//! Dispatch order: `topmost()` from `OverlayManager` determines which overlay
//! captures input. Each overlay is handled by a dedicated private function.

use crate::app::{AppState, InputEvent, OutputEvent};
use crate::handlers::HandlerContext;
use crate::handlers::input_commands::{dispatch_builtin_command, execute_shortcuts_command};
use crate::handlers::{
    approval, changeset as changeset_handler, file_search, isolation_switcher, message_action,
    model_switcher, profile_switcher, review as review_handler, rulebook_switcher,
};
use crate::overlay::OverlayId;
use tokio::sync::mpsc::Sender;

/// Dispatch event to the topmost active overlay. Always consumes the event.
pub fn dispatch_popup_event(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) -> bool {
    match state.overlay_manager.topmost() {
        Some(OverlayId::RejectReason) => { handle_reject_reason(state, output_tx, event); true }
        Some(OverlayId::AskUser) => { handle_ask_user(state, output_tx, event); true }
        Some(OverlayId::PlanReview) => { crate::handlers::plan::handle_plan_review_key(state, &event); true }
        Some(OverlayId::FileChanges) => { handle_file_changes(state, output_tx, event); true }
        Some(OverlayId::HelperDropdown) => { handle_helper_dropdown(state, event); true }
        Some(OverlayId::AtDropdown) => { handle_at_dropdown(state, event); true }
        Some(OverlayId::CommandPalette) => { handle_command_palette(state, output_tx, event); true }
        Some(OverlayId::Shortcuts) => { handle_shortcuts(state, output_tx, event); true }
        Some(OverlayId::IsolationSwitcher) => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = isolation_switcher::handle_event(&mut ctx, event);
            true
        }
        Some(OverlayId::ProfileSwitcher) => { handle_profile_switcher(state, output_tx, event); true }
        Some(OverlayId::RulebookSwitcher) => { handle_rulebook_switcher(state, output_tx, event); true }
        Some(OverlayId::MessageAction) => {
            let mut ctx = HandlerContext::new(state, output_tx);
            let _ = message_action::handle_event(&mut ctx, event);
            true
        }
        Some(OverlayId::ModelSwitcher) => { handle_model_switcher(state, output_tx, event); true }
        Some(OverlayId::FileSearch) => { handle_file_search(state, output_tx, event); true }
        Some(OverlayId::Changeset) => { handle_changeset(state, output_tx, event); true }
        Some(OverlayId::ReviewPane) => { handle_review_pane(state, output_tx, event); true }
        Some(OverlayId::ShellPopup) => {
            crate::handlers::shell::handle_shell_key(state, output_tx, &event);
            true
        }
        None => false,
    }
}

// ── Per-overlay handlers ─────────────────────────────────────────────────────

fn handle_reject_reason(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::InputSubmitted => { let _ = approval::confirm_reject_current(&mut ctx); }
        InputEvent::HandleEsc => {
            ctx.state.reject_reason_input = None;
            ctx.state.overlay_manager.pop(OverlayId::RejectReason);
            let _ = approval::reject_current(&mut ctx);
        }
        InputEvent::InputChanged(c) => { let _ = approval::reason_input_push(&mut ctx, c); }
        InputEvent::InputBackspace => { let _ = approval::reason_input_pop(&mut ctx); }
        _ => {}
    }
}

fn handle_file_changes(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    let filtered = crate::services::file_changes_popup::filtered_paths(state);
    match event {
        InputEvent::HandleEsc => { crate::overlay::close_overlay(state, OverlayId::FileChanges); }
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

fn handle_helper_dropdown(state: &mut AppState, event: InputEvent) {
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
        InputEvent::HandleEsc => { crate::overlay::close_overlay(state, OverlayId::HelperDropdown); }
        _ => {}
    }
}

fn handle_at_dropdown(state: &mut AppState, event: InputEvent) {
    if state.focus != crate::app::WorkspaceFocus::Input {
        return;
    }
    match event {
        InputEvent::Up => {
            state.at_selected_idx = state.at_selected_idx.saturating_sub(1);
        }
        InputEvent::Down => {
            if !state.at_results.is_empty() {
                state.at_selected_idx =
                    (state.at_selected_idx + 1).min(state.at_results.len().saturating_sub(1));
            }
        }
        InputEvent::InputSubmitted => {
            if let Some(path) = state.at_results.get(state.at_selected_idx).cloned() {
                let remove_len = state.at_query.len() + 1;
                for _ in 0..remove_len { state.input.backspace(); }
                state.input.insert_str(&path);
                state.input.input(' ');
            }
            crate::overlay::close_overlay(state, OverlayId::AtDropdown);
        }
        InputEvent::HandleEsc => {
            crate::overlay::close_overlay(state, OverlayId::AtDropdown);
        }
        InputEvent::InputChanged(' ') => {
            state.input.input(' ');
            crate::overlay::close_overlay(state, OverlayId::AtDropdown);
        }
        InputEvent::InputChanged(c) => {
            state.at_query.push(c);
            state.at_selected_idx = 0;
            state.at_results = crate::services::fuzzy_search_files(&state.at_query, &state.all_files, 8);
            state.input.input(c);
        }
        InputEvent::InputBackspace => {
            if state.at_query.is_empty() {
                crate::overlay::close_overlay(state, OverlayId::AtDropdown);
            } else {
                state.at_query.pop();
                state.at_selected_idx = 0;
                state.at_results =
                    crate::services::fuzzy_search_files(&state.at_query, &state.all_files, 8);
            }
            state.input.backspace();
        }
        _ => {}
    }
}

fn handle_command_palette(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
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

fn handle_shortcuts(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    match event {
        InputEvent::HandleEsc | InputEvent::HideShortcuts => {
            crate::overlay::close_overlay(state, OverlayId::Shortcuts);
        }
        InputEvent::Tab => {
            state.shortcuts_mode = match state.shortcuts_mode {
                crate::app::ShortcutsPopupMode::Commands => crate::app::ShortcutsPopupMode::Shortcuts,
                crate::app::ShortcutsPopupMode::Shortcuts => crate::app::ShortcutsPopupMode::Sessions,
                crate::app::ShortcutsPopupMode::Sessions => crate::app::ShortcutsPopupMode::Commands,
            };
            state.shortcuts_scroll = 0;
        }
        InputEvent::Up => { state.shortcuts_scroll = state.shortcuts_scroll.saturating_sub(1); }
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
            if state.shortcuts_scroll < max { state.shortcuts_scroll += 1; }
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
                    if !keep_open { crate::overlay::close_overlay(state, OverlayId::Shortcuts); }
                }
            }
        }
        _ => {}
    }
}

fn handle_profile_switcher(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::HandleEsc => { let _ = profile_switcher::close(&mut ctx); }
        InputEvent::InputChanged(c) => {
            let mut f = ctx.state.profile_search_input.clone();
            f.push(c);
            let _ = profile_switcher::update_filter(&mut ctx, f);
        }
        InputEvent::InputBackspace => {
            let mut f = ctx.state.profile_search_input.clone();
            f.pop();
            let _ = profile_switcher::update_filter(&mut ctx, f);
        }
        InputEvent::Up => { let _ = profile_switcher::select_prev(&mut ctx); }
        InputEvent::Down => { let _ = profile_switcher::select_next(&mut ctx); }
        InputEvent::InputSubmitted => { let _ = profile_switcher::submit_selected(&mut ctx); }
        _ => {}
    }
}

fn handle_rulebook_switcher(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::HandleEsc => { let _ = rulebook_switcher::close(&mut ctx); }
        InputEvent::InputChanged(c) => {
            if c == ' ' {
                let _ = rulebook_switcher::toggle_selected(&mut ctx);
            } else {
                let mut f = ctx.state.rulebook_search_input.clone();
                f.push(c);
                let _ = rulebook_switcher::update_filter(&mut ctx, f);
            }
        }
        InputEvent::InputBackspace => {
            let mut f = ctx.state.rulebook_search_input.clone();
            f.pop();
            let _ = rulebook_switcher::update_filter(&mut ctx, f);
        }
        InputEvent::Up => { let _ = rulebook_switcher::select_prev(&mut ctx); }
        InputEvent::Down => { let _ = rulebook_switcher::select_next(&mut ctx); }
        InputEvent::InputSubmitted => { let _ = rulebook_switcher::submit_selected(&mut ctx); }
        _ => {}
    }
}

fn handle_model_switcher(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::HandleEsc => { let _ = model_switcher::close(&mut ctx); }
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
        InputEvent::Up => { let _ = model_switcher::select_prev(&mut ctx); }
        InputEvent::Down => { let _ = model_switcher::select_next(&mut ctx); }
        InputEvent::InputSubmitted => { let _ = model_switcher::submit_selected(&mut ctx); }
        _ => {}
    }
}

fn handle_file_search(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    if state.all_files.is_empty() {
        state.all_files = crate::services::build_file_index(&state.project_root);
    }
    if state.file_search_results.is_empty() {
        let q = state.file_search_query.clone();
        let results = crate::services::fuzzy_search_files(&q, &state.all_files, 50);
        let max = results.len().saturating_sub(1);
        state.file_search_results = results;
        state.file_search_selected_idx = state.file_search_selected_idx.min(max);
    }
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::HandleEsc => { let _ = file_search::close(&mut ctx); }
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
        InputEvent::Up => { let _ = file_search::select_prev(&mut ctx); }
        InputEvent::Down => { let _ = file_search::select_next(&mut ctx); }
        InputEvent::InputSubmitted => { let _ = file_search::insert_selected(&mut ctx); }
        _ => {}
    }
}

fn handle_changeset(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::HandleEsc => { let _ = changeset_handler::close(&mut ctx); }
        InputEvent::Up => { let _ = changeset_handler::select_prev(&mut ctx); }
        InputEvent::Down => { let _ = changeset_handler::select_next(&mut ctx); }
        InputEvent::ScrollUp => { let _ = changeset_handler::scroll_up(&mut ctx); }
        InputEvent::ScrollDown => { let _ = changeset_handler::scroll_down(&mut ctx); }
        _ => {}
    }
}

fn handle_review_pane(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::HandleEsc | InputEvent::ReviewClose => { let _ = review_handler::close(&mut ctx); }
        InputEvent::ReviewOpen => { let _ = review_handler::open(&mut ctx); }
        InputEvent::ReviewUp | InputEvent::Up | InputEvent::ScrollUp => { let _ = review_handler::select_prev(&mut ctx); }
        InputEvent::ReviewDown | InputEvent::Down | InputEvent::ScrollDown => { let _ = review_handler::select_next(&mut ctx); }
        InputEvent::ReviewFilterInput(c) | InputEvent::InputChanged(c) => { let _ = review_handler::filter_push(&mut ctx, c); }
        InputEvent::ReviewFilterBackspace | InputEvent::InputBackspace => { let _ = review_handler::filter_pop(&mut ctx); }
        InputEvent::ReviewToggleDiff | InputEvent::InputSubmitted => { let _ = review_handler::toggle_diff(&mut ctx); }
        InputEvent::PageUp => { let _ = review_handler::scroll_up(&mut ctx, 10); }
        InputEvent::PageDown => { let _ = review_handler::scroll_down(&mut ctx, 10); }
        InputEvent::ReviewRevertSelected => { let _ = review_handler::revert_selected(&mut ctx); }
        InputEvent::ReviewRevertFiltered => { let _ = review_handler::revert_filtered(&mut ctx); }
        InputEvent::ReviewRevertAll | InputEvent::HandleCtrlZ => { let _ = review_handler::revert_all(&mut ctx); }
        InputEvent::ReviewOpenEditor => { let _ = review_handler::open_editor(&mut ctx); }
        _ => {}
    }
}

// ── Ask-user handler (kept here because of its size) ────────────────────────

fn handle_ask_user(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    {
        let kind = state.ask_user_question_kind;
        let has_options = !state.ask_user_options.is_empty();
        let max_visible = 10usize;
        let filtered = crate::services::ask_user::filtered_option_indices(
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
                    let tool_call = crate::types::ToolCall {
                        id: tc_id.clone(),
                        r#type: "function".to_string(),
                        function: crate::types::FunctionCall {
                            name: crate::services::ask_user::ASK_USER_TOOL_NAME.to_string(),
                            arguments: String::new(),
                        },
                        metadata: None,
                    };
                    let result = crate::types::ToolCallResult {
                        call: tool_call,
                        result: "User cancelled.".to_string(),
                        status: crate::types::ToolCallResultStatus::Error,
                    };
                    let _ =
                        output_tx.try_send(OutputEvent::SendToolResult(result, false, Vec::new()));
                }
                crate::overlay::close_overlay(state, OverlayId::AskUser);
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
                    let filtered = crate::services::ask_user::filtered_option_indices(
                        &state.ask_user_filter,
                        &state.ask_user_options,
                    );
                    if let Some(&first) = filtered.first() {
                        state.ask_user_selected = first;
                    }
                    return;
                }

                if kind == crate::services::ask_user::AskUserQuestionKind::MultiSelect
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
                    let filtered = crate::services::ask_user::filtered_option_indices(
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
                if kind == crate::services::ask_user::AskUserQuestionKind::MultiSelect {
                    state.ask_user_multi_selected.clear();
                } else if state.ask_user_allow_free_text {
                    state.ask_user_input.clear();
                }
                return;
            }
            InputEvent::InputCursorStart => {
                if kind == crate::services::ask_user::AskUserQuestionKind::MultiSelect {
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
                    let tool_call = crate::types::ToolCall {
                        id: tc_id.clone(),
                        r#type: "function".to_string(),
                        function: crate::types::FunctionCall {
                            name: crate::services::ask_user::ASK_USER_TOOL_NAME.to_string(),
                            arguments: String::new(),
                        },
                        metadata: None,
                    };
                    let payload = crate::services::ask_user::build_answer(
                        state,
                        &state.ask_user_multi_selected,
                    );
                    let answer =
                        serde_json::to_string(&payload).unwrap_or_else(|_| payload.to_string());
                    let result = crate::types::ToolCallResult {
                        call: tool_call,
                        result: answer.clone(),
                        status: crate::types::ToolCallResultStatus::Success,
                    };
                    let _ =
                        output_tx.try_send(OutputEvent::SendToolResult(result, false, Vec::new()));
                    let question = state.ask_user_question.clone().unwrap_or_default();
                    let summary = crate::services::ask_user::answer_summary(state, &filtered);
                    state.push_activity(
                        crate::app::ActivityKind::Approval,
                        crate::services::ask_user::transcript_annotation(&question, &summary),
                    );
                }
                crate::overlay::close_overlay(state, OverlayId::AskUser);
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

}
