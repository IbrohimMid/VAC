//! @-mention overlay input handler (PR-T7).

use crate::app::{AppState, InputEvent};
use crate::overlay::OverlayId;

pub(super) fn handle_at_dropdown(state: &mut AppState, event: InputEvent) {
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
                // Remove the @<query> from the input buffer
                let remove_len = state.at_query.len() + 1; // +1 for '@'
                for _ in 0..remove_len {
                    state.input.backspace();
                }
                // Detect namespace prefix: @@skill, @#todo, @!session
                let (namespace, label) = parse_at_namespace(&state.at_query, &path);
                let content = match namespace {
                    crate::app::types::ChipNamespace::File => std::fs::read_to_string(&path)
                        .unwrap_or_else(|_| format!("(could not read {path})")),
                    _ => path.clone(),
                };
                state.context_chips.push(crate::app::types::ContextChip {
                    label,
                    content,
                    namespace,
                });
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
            state.at_results =
                crate::services::fuzzy_search_files(&state.at_query, &state.all_files, 8);
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

pub(super) fn parse_at_namespace(
    query: &str,
    path: &str,
) -> (crate::app::types::ChipNamespace, String) {
    use crate::app::types::ChipNamespace;
    if query.starts_with('@') {
        (ChipNamespace::Skill, path.to_string())
    } else if query.starts_with('#') {
        (ChipNamespace::Todo, path.to_string())
    } else if query.starts_with('!') {
        (ChipNamespace::Session, path.to_string())
    } else {
        let label = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
            .to_string();
        (ChipNamespace::File, label)
    }
}
