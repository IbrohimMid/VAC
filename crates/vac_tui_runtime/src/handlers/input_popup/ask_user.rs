//! Ask-user overlay handler (approval/selection dialogs).

use crate::app::{AppState, InputEvent, OutputEvent};
use crate::overlay::OverlayId;
use tokio::sync::mpsc::Sender;

/// Handle ask-user question interactions (single-select, multi-select, free-text).
pub fn handle_ask_user(state: &mut AppState, output_tx: &Sender<OutputEvent>, event: InputEvent) {
    {
        let kind = state.ask_user.question_kind;
        let has_options = !state.ask_user.options.is_empty();
        let max_visible = 10usize;
        let filtered = crate::services::ask_user::filtered_option_indices(
            &state.ask_user.filter,
            &state.ask_user.options,
        );
        let selected_pos = if filtered.is_empty() {
            0
        } else {
            filtered
                .iter()
                .position(|&i| i == state.ask_user.selected)
                .unwrap_or(0)
        };

        match event {
            InputEvent::HandleEsc => {
                // Cancel: send an error-status tool result and close popup.
                if let Some(tc_id) = state.ask_user.tool_call_id.take() {
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
                state.ask_user.question = None;
                state.ask_user.options.clear();
                state.ask_user.input.clear();
                state.ask_user.multi_selected.clear();
                state.ask_user.metadata.clear();
                state.ask_user.filter.clear();
                state.ask_user.search_active = false;
                state.ask_user.scroll = 0;
            }
            InputEvent::Tab => {
                if has_options {
                    state.ask_user.search_active = !state.ask_user.search_active;
                }
            }
            InputEvent::Up | InputEvent::ScrollUp => {
                if !filtered.is_empty() && selected_pos > 0 {
                    let new_pos = selected_pos - 1;
                    state.ask_user.selected = filtered[new_pos];
                    state.ask_user.scroll = state.ask_user.scroll.min(new_pos);
                }
            }
            InputEvent::Down | InputEvent::ScrollDown => {
                if !filtered.is_empty() && selected_pos + 1 < filtered.len() {
                    let new_pos = selected_pos + 1;
                    state.ask_user.selected = filtered[new_pos];
                    let max_scroll = filtered.len().saturating_sub(1);
                    state.ask_user.scroll = state.ask_user.scroll.min(max_scroll);
                    if new_pos >= state.ask_user.scroll.saturating_add(max_visible) {
                        state.ask_user.scroll = new_pos + 1 - max_visible;
                    }
                }
            }
            InputEvent::InputChanged(c) => {
                if state.ask_user.search_active {
                    state.ask_user.filter.push(c);
                    state.ask_user.scroll = 0;
                    let filtered = crate::services::ask_user::filtered_option_indices(
                        &state.ask_user.filter,
                        &state.ask_user.options,
                    );
                    if let Some(&first) = filtered.first() {
                        state.ask_user.selected = first;
                    }
                } else if kind == crate::services::ask_user::AskUserQuestionKind::MultiSelect
                    && c == ' '
                    && !filtered.is_empty()
                {
                    if state
                        .ask_user.multi_selected
                        .contains(&state.ask_user.selected)
                    {
                        state
                            .ask_user.multi_selected
                            .remove(&state.ask_user.selected);
                    } else {
                        state
                            .ask_user.multi_selected
                            .insert(state.ask_user.selected);
                    }
                } else {
                    // Number shortcut: 1-9 selects options[n-1] when free-text is empty.
                    if state.ask_user.input.is_empty() && c.is_ascii_digit() && c != '0' {
                        let idx = (c as u8 - b'1') as usize;
                        if idx < filtered.len() {
                            state.ask_user.selected = filtered[idx];
                            return;
                        }
                    }
                    // Honor `allow_free_text`: when the caller disabled it,
                    // typed characters that aren't number shortcuts are dropped.
                    if state.ask_user.allow_free_text {
                        state.ask_user.input.push(c);
                    }
                }
            }
            InputEvent::InputBackspace => {
                if state.ask_user.search_active {
                    state.ask_user.filter.pop();
                    state.ask_user.scroll = 0;
                    let filtered = crate::services::ask_user::filtered_option_indices(
                        &state.ask_user.filter,
                        &state.ask_user.options,
                    );
                    if let Some(&first) = filtered.first() {
                        state.ask_user.selected = first;
                    }
                } else if state.ask_user.allow_free_text {
                    state.ask_user.input.pop();
                }
            }
            InputEvent::InputClear => {
                if state.ask_user.search_active {
                    state.ask_user.filter.clear();
                    state.ask_user.scroll = 0;
                } else if kind == crate::services::ask_user::AskUserQuestionKind::MultiSelect {
                    state.ask_user.multi_selected.clear();
                } else if state.ask_user.allow_free_text {
                    state.ask_user.input.clear();
                }
            }
            InputEvent::InputCursorStart => {
                if kind == crate::services::ask_user::AskUserQuestionKind::MultiSelect {
                    let indices = if state.ask_user.filter.trim().is_empty() {
                        (0..state.ask_user.options.len()).collect::<Vec<_>>()
                    } else {
                        filtered.clone()
                    };
                    state.ask_user.multi_selected = indices.into_iter().collect();
                }
            }
            InputEvent::InputSubmitted => {
                if let Some(tc_id) = state.ask_user.tool_call_id.take() {
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
                        &state.ask_user.multi_selected,
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
                    let question = state.ask_user.question.clone().unwrap_or_default();
                    let summary = crate::services::ask_user::answer_summary(state, &filtered);
                    state.push_activity(
                        crate::app::ActivityKind::Approval,
                        crate::services::ask_user::transcript_annotation(&question, &summary),
                    );
                }
                crate::overlay::close_overlay(state, OverlayId::AskUser);
                state.ask_user.question = None;
                state.ask_user.options.clear();
                state.ask_user.input.clear();
                state.ask_user.multi_selected.clear();
                state.ask_user.metadata.clear();
                state.ask_user.filter.clear();
                state.ask_user.search_active = false;
                state.ask_user.scroll = 0;
            }
            _ => {}
        }
    }
}
